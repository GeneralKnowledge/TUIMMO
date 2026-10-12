//! Private mail. The recipient owns the row. The sender does not keep a copy.

use terminus_core::{
    Audience, Channel, CommandOutcome, GameError, ParsedCommand, PlayerId, SessionId,
};

use crate::mail::{
    clean_body, format_id, format_received, parse_mail_id, preview, send_is_too_soon, MailMessage,
    BODY_MAX, INBOX_CAP, MAIL_SEND_INTERVAL_MS,
};
use crate::model::PersistOp;
use crate::state::{quiet, Transition, WorldState};

impl WorldState {
    pub(crate) fn cmd_mail(
        &mut self,
        session_id: SessionId,
        cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let rest = cmd.rest.trim();
        if rest.is_empty() {
            return self.mail_summary(session_id);
        }
        let mut parts = rest.splitn(2, char::is_whitespace);
        let head = parts.next().unwrap_or("").to_ascii_lowercase();
        let tail = parts.next().unwrap_or("").trim();
        match head.as_str() {
            "list" => {
                if !tail.is_empty() {
                    return Err(GameError::Rejected("usage: mail list".into()));
                }
                self.mail_list(session_id)
            }
            "read" => self.mail_read(session_id, tail),
            "send" => self.mail_send(session_id, tail),
            "delete" => self.mail_delete(session_id, tail),
            _ => Err(GameError::Rejected("usage: mail [list|read|send|delete]".into())),
        }
    }

    fn mail_summary(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let total = self.mailbox.count_for(player);
        let unread = self.mailbox.unread_for(player);
        let summary = if total == 0 {
            "Mailbox: no mail.".to_string()
        } else {
            format!("Mailbox: {unread} unread of {total}.")
        };
        self.set_overlay(session_id, format!("{}\n{summary}", mail_help()));
        Ok(quiet())
    }

    fn mail_list(&mut self, session_id: SessionId) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let rows = self.mailbox.inbox(player);
        if rows.is_empty() {
            self.set_overlay(session_id, "No mail has arrived.\n`look` returns to the place.");
            return Ok(quiet());
        }
        let unread = rows.iter().filter(|message| message.read_at.is_none()).count();
        let mut lines = vec![format!("Mailbox ({}, {unread} unread)", rows.len())];
        for message in rows {
            let sender = self.operator_label(message.sender_id);
            let status = if message.read_at.is_none() { "Unread" } else { "Read" };
            lines.push(format!(
                "{}  {sender}  {}  {status}",
                format_id(message.id),
                format_received(message.created_at)
            ));
            lines.push(format!("  {}", preview(&message.body)));
        }
        lines.push("`mail read <id>`".into());
        lines.push("`look` returns to the place.".into());
        self.set_overlay(session_id, lines.join("\n"));
        Ok(quiet())
    }

    fn mail_read(&mut self, session_id: SessionId, raw_id: &str) -> Result<Transition, GameError> {
        if raw_id.is_empty() || raw_id.split_whitespace().nth(1).is_some() {
            return Err(GameError::Rejected("usage: mail read <id>".into()));
        }
        let id = parse_mail_id(raw_id).map_err(|message| GameError::Rejected(message.into()))?;
        let player = self.player(session_id)?;
        let Some(message) = self.mailbox.owned(player, id).cloned() else {
            return Err(GameError::Rejected(format!(
                "No message {} in your mailbox.",
                format_id(id)
            )));
        };
        let was_unread = message.read_at.is_none();
        let status = if was_unread { "Unread" } else { "Read" };
        let page = format!(
            "Mail {}\nFrom: {}\nReceived: {}\nStatus: {status}\n{}\n`look` returns to the place.",
            format_id(message.id),
            self.operator_label(message.sender_id),
            format_received(message.created_at),
            message.body
        );
        self.set_overlay(session_id, page);
        if !was_unread {
            return Ok(quiet());
        }
        let read_at = self.now_ms;
        if let Some(row) =
            self.mailbox.messages.iter_mut().find(|row| row.id == id && row.recipient_id == player)
        {
            row.read_at = Some(read_at);
        }
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("Marked mail {} as read.", format_id(id)),
        );
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![PersistOp::MailRead { id, recipient_id: player, read_at }, note],
        })
    }

    fn mail_send(&mut self, session_id: SessionId, tail: &str) -> Result<Transition, GameError> {
        self.require_speak(session_id)?;
        let mut parts = tail.splitn(2, char::is_whitespace);
        let who = parts.next().unwrap_or("").trim();
        let raw_body = parts.next().unwrap_or("").trim();
        if who.is_empty() {
            return Err(GameError::Rejected("Who?".into()));
        }
        let sender = self.player(session_id)?;
        let recipient = self.resolve_player_name(who)?;
        if recipient == sender {
            return Err(GameError::Rejected("You cannot mail yourself.".into()));
        }
        let body = clean_body(raw_body).map_err(|message| GameError::Rejected(message.into()))?;
        if self.social.blocked_either_way(sender, recipient) {
            return Err(GameError::Rejected(
                "You cannot message that operator (ignore/block is in effect).".into(),
            ));
        }
        if self.mailbox.count_for(recipient) >= INBOX_CAP {
            return Err(GameError::Rejected("That mailbox is full.".into()));
        }
        if send_is_too_soon(self.mailbox.last_send_at.get(&sender).copied(), self.now_ms) {
            return Err(GameError::Rejected("Wait a moment before sending more mail.".into()));
        }
        if self.mailbox.next_id == 0 || self.mailbox.next_id > i64::MAX as u64 {
            return Err(GameError::Rejected("The mailbox cannot accept another letter.".into()));
        }
        let id = self.mailbox.next_id;
        let created_at = self.now_ms;
        let sender_name = self.operator_label(sender);
        let recipient_name = self.operator_label(recipient);
        let waiting = self.is_online(recipient);
        self.mailbox.next_id = self.mailbox.next_id.saturating_add(1);
        self.mailbox.last_send_at.insert(sender, created_at);
        self.mailbox.messages.push(MailMessage {
            id,
            sender_id: sender,
            recipient_id: recipient,
            body: body.clone(),
            created_at,
            read_at: None,
        });
        let mut events = Vec::new();
        let mut ops = vec![PersistOp::MailInsert {
            id,
            sender_id: sender,
            recipient_id: recipient,
            body,
            created_at,
        }];
        let sent = if waiting {
            format!("Sent mail {} to {recipient_name}.", format_id(id))
        } else {
            format!("Sent mail {} to {recipient_name}. It waits until they return.", format_id(id))
        };
        let (to_self, op_self) = self.emit(Channel::System, Audience::Player(sender), None, sent);
        events.push(to_self);
        ops.push(op_self);
        if waiting {
            let (to_them, op_them) = self.emit(
                Channel::System,
                Audience::Player(recipient),
                None,
                format!("Mail from {sender_name}. `mail read {}`.", format_id(id)),
            );
            events.push(to_them);
            ops.push(op_them);
        }
        Ok(Transition { outcome: CommandOutcome { events, quit: false }, ops })
    }

    fn mail_delete(
        &mut self,
        session_id: SessionId,
        raw_id: &str,
    ) -> Result<Transition, GameError> {
        self.require_play(session_id)?;
        if raw_id.is_empty() || raw_id.split_whitespace().nth(1).is_some() {
            return Err(GameError::Rejected("usage: mail delete <id>".into()));
        }
        let id = parse_mail_id(raw_id).map_err(|message| GameError::Rejected(message.into()))?;
        let player = self.player(session_id)?;
        if self.mailbox.owned(player, id).is_none() {
            return Err(GameError::Rejected(format!(
                "No message {} in your mailbox.",
                format_id(id)
            )));
        }
        self.mailbox
            .messages
            .retain(|message| !(message.id == id && message.recipient_id == player));
        let (event, note) = self.emit(
            Channel::System,
            Audience::Player(player),
            None,
            format!("Deleted mail {}.", format_id(id)),
        );
        self.mail_list(session_id)?;
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![PersistOp::MailDelete { id, recipient_id: player }, note],
        })
    }

    fn require_speak(&self, session_id: SessionId) -> Result<(), GameError> {
        if self.permissions(session_id)?.can_speak() {
            Ok(())
        } else {
            Err(GameError::Rejected("You cannot speak right now.".into()))
        }
    }

    fn operator_label(&self, player: PlayerId) -> String {
        self.operators
            .get(&player)
            .map(|op| op.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "unknown".into())
    }
}

fn mail_help() -> String {
    let seconds = MAIL_SEND_INTERVAL_MS / 1000;
    format!(
        "\
Mail
A private letter to one operator. It is kept until they delete it.
Whisper is live and is not stored. Mail still arrives when they are offline.

mail
mail list
mail read <id>
mail send <username> <message>
mail delete <id>

Example:
mail send bob I found the old relay. The dish still faces the gate.

Ids are numbers, such as 42 or #42. They stay the same across a restart and are not reused.
A letter is at most {BODY_MAX} characters. A mailbox holds {INBOX_CAP} letters.
A full mailbox refuses the new letter. Nothing already there is deleted.
One letter every {seconds} seconds. The gap resets when the server restarts.

The name is the same token whisper uses: username or display name, ignoring case.
You cannot mail yourself. Sending does not create an account.
Ignore in either direction refuses delivery, the same as whisper. The refusal does not say which side ignored the other.
Mail already delivered stays in the mailbox. There is no sent folder, and the letter is not posted to chat, boards, or journals.

`look` returns to the place."
    )
}

#[cfg(test)]
mod tests {
    use terminus_core::{Game, Permissions, SessionIdentity};
    use uuid::Uuid;

    use super::*;
    use crate::mail::{Mailbox, BODY_MAX, INBOX_CAP, MAIL_SEND_INTERVAL_MS};
    use crate::state::starter_operator;

    fn pair() -> (WorldState, SessionId, SessionId, PlayerId, PlayerId) {
        let mut world = WorldState::for_tests();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        world.insert_operator(starter_operator(ada_id, Uuid::new_v4(), "ada", "Ada"));
        world.insert_operator(starter_operator(bob_id, Uuid::new_v4(), "bob", "Robert"));
        let ada = SessionIdentity::new(ada_id, "Ada", Permissions::player());
        let bob = SessionIdentity::new(bob_id, "Robert", Permissions::player());
        let ada_session = ada.session_id;
        let bob_session = bob.session_id;
        world.connect_player(ada).unwrap();
        world.connect_player(bob).unwrap();
        (world, ada_session, bob_session, ada_id, bob_id)
    }

    fn overlay(world: &WorldState, session: SessionId) -> String {
        world.view(session).unwrap().overlay.clone().unwrap_or_default()
    }

    fn feed(world: &WorldState, session: SessionId) -> String {
        world
            .view(session)
            .unwrap()
            .feed
            .iter()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn advance(world: &mut WorldState) {
        world.now_ms = world.now_ms.saturating_add(MAIL_SEND_INTERVAL_MS);
    }

    #[test]
    fn help_and_an_empty_mailbox() {
        let (mut world, ada, _, _, _) = pair();
        world.apply(ada, "mail").unwrap();
        let help = overlay(&world, ada);
        assert!(help.contains("mail send <username>"));
        assert!(help.contains("42 or #42"));
        assert!(help.contains(&BODY_MAX.to_string()));
        assert!(help.contains(&INBOX_CAP.to_string()));
        assert!(help.contains("Mailbox: no mail."));
        assert!(help.contains("Whisper is live"));
        world.apply(ada, "mail list").unwrap();
        assert!(overlay(&world, ada).contains("No mail has arrived."));
        world.apply(ada, "help mail").unwrap();
        let topic = overlay(&world, ada);
        assert!(topic.contains("mail [list|read|send|delete]"));
    }

    #[test]
    fn send_list_read_and_delete_stay_with_the_recipient() {
        let (mut world, ada, bob, _, bob_id) = pair();
        let token = "zinc-token-441";
        let sent = world
            .apply(ada, &format!("mail send BoB The dish still faces the gate. {token}"))
            .unwrap();
        assert_eq!(
            sent.ops.iter().filter(|op| matches!(op, PersistOp::MailInsert { .. })).count(),
            1
        );
        assert!(feed(&world, ada).contains("Sent mail #1 to Robert."));
        assert!(!feed(&world, ada).contains(token));
        assert!(feed(&world, bob).contains("`mail read #1`"));
        assert!(!feed(&world, bob).contains(token));
        assert_eq!(world.mailbox.count_for(bob_id), 1);

        advance(&mut world);
        world.apply(ada, "mail send robert A later note.").unwrap();
        world.apply(bob, "mail list").unwrap();
        let list = overlay(&world, bob);
        let first = list.lines().position(|line| line.starts_with("#2")).unwrap();
        let second = list.lines().position(|line| line.starts_with("#1")).unwrap();
        assert!(first < second, "{list}");
        assert!(list.contains("Unread"));
        assert!(list.contains(token));

        world.apply(ada, "mail list").unwrap();
        assert!(overlay(&world, ada).contains("No mail has arrived."));
        let err = world.apply(ada, "mail read 1").unwrap_err();
        assert!(err.to_string().contains("No message #1"), "{err}");
        assert!(world.mailbox.owned(bob_id, 1).unwrap().read_at.is_none());

        world.apply(bob, "mail read #1").unwrap();
        let page = overlay(&world, bob);
        assert!(page.contains("From: Ada"));
        assert!(page.contains("Status: Unread"));
        assert!(page.contains(token));
        assert!(feed(&world, bob).contains("Marked mail #1 as read."));
        assert!(world.mailbox.owned(bob_id, 1).unwrap().read_at.is_some());
        assert!(world.view(bob).unwrap().status.iter().any(|line| line == "mail: 1 unread"));

        world.apply(bob, "mail read 1").unwrap();
        assert!(overlay(&world, bob).contains("Status: Read"));
        world.apply(bob, "mail delete 2").unwrap();
        assert!(world.mailbox.owned(bob_id, 2).is_none());
        assert!(world.mailbox.owned(bob_id, 1).is_some());
        assert!(feed(&world, bob).contains("Deleted mail #2."));
        assert!(!overlay(&world, bob).contains("#2"));
        assert_eq!(world.mailbox.next_id, 3);
    }

    #[test]
    fn offline_delivery_and_whisper_still_refuses() {
        let (mut world, ada, bob, _, bob_id) = pair();
        world.detach(bob).unwrap();
        let err = world.apply(ada, "whisper bob later").unwrap_err();
        assert!(err.to_string().contains("offline"), "{err}");
        assert!(err.to_string().contains("not stored"), "{err}");
        world.apply(ada, "mail send bob waiting on the relay").unwrap();
        assert!(feed(&world, ada).contains("It waits until they return."));
        assert_eq!(world.mailbox.unread_for(bob_id), 1);
        let bob = SessionIdentity::new(bob_id, "Robert", Permissions::player());
        let bob_session = bob.session_id;
        world.connect_player(bob).unwrap();
        world.apply(bob_session, "mail list").unwrap();
        let list = overlay(&world, bob_session);
        assert!(list.contains("waiting on the relay"));
        assert!(list.contains("Unread"));
        assert!(world
            .view(bob_session)
            .unwrap()
            .status
            .iter()
            .any(|line| line.contains("mail: 1 unread")));
    }

    #[test]
    fn strangers_cannot_read_or_delete_by_id_and_chat_stays_clean() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        let cy_id = PlayerId::new();
        world.insert_operator(starter_operator(cy_id, Uuid::new_v4(), "cy", "Cy"));
        let cy = SessionIdentity::new(cy_id, "Cy", Permissions::player());
        let cy_session = cy.session_id;
        world.connect_player(cy).unwrap();
        let token = "zinc-token-441";
        world.apply(ada, "say the room is quiet").unwrap();
        world.apply(ada, &format!("mail send bob {token}")).unwrap();
        world.apply(ada, "chat nothing to see").unwrap();
        world.apply(ada, "region a regional line").unwrap();

        let cy_feed = feed(&world, cy_session);
        assert!(cy_feed.contains("the room is quiet"), "{cy_feed}");
        assert!(cy_feed.contains("nothing to see"), "{cy_feed}");
        assert!(cy_feed.contains("a regional line"), "{cy_feed}");
        assert!(!cy_feed.contains(token), "{cy_feed}");
        assert!(!feed(&world, ada).contains(token));
        assert!(!feed(&world, bob).contains(token));

        world.apply(cy_session, "mail list").unwrap();
        assert!(overlay(&world, cy_session).contains("No mail has arrived."));
        for session in [ada, cy_session] {
            let err = world.apply(session, "mail read 1").unwrap_err();
            assert!(err.to_string().contains("No message #1"), "{err}");
            assert!(!err.to_string().contains(token));
            let err = world.apply(session, "mail delete #1").unwrap_err();
            assert!(err.to_string().contains("No message #1"), "{err}");
        }
        assert!(world.mailbox.owned(bob_id, 1).is_some());
        assert!(world.mailbox.owned(ada_id, 1).is_none());
        world.apply(bob, "mail delete 1").unwrap();
        assert!(world.mailbox.messages.is_empty());
    }

    #[test]
    fn rejects_self_unknown_empty_and_overlong_bodies() {
        let (mut world, ada, _, _, _) = pair();
        let err = world.apply(ada, "mail send ada hello").unwrap_err();
        assert!(err.to_string().contains("yourself"), "{err}");
        let err = world.apply(ada, "mail send nobody hello").unwrap_err();
        assert!(err.to_string().contains("No operator"), "{err}");
        let err = world.apply(ada, "mail send bob   ").unwrap_err();
        assert!(err.to_string().contains("Write a message"), "{err}");
        let err =
            world.apply(ada, &format!("mail send bob {}", "a".repeat(BODY_MAX + 1))).unwrap_err();
        assert!(err.to_string().contains("400"), "{err}");
        assert!(world.mailbox.messages.is_empty());
        let err = world.apply(ada, "post bob hello").unwrap_err();
        assert!(err.to_string().contains("Unknown command"), "{err}");
        assert!(world.apply(ada, "mail nosuch").is_err());
    }

    #[test]
    fn full_mailbox_rejects_and_a_fast_second_send_waits() {
        let (mut world, ada, _bob, _, bob_id) = pair();
        let cy_id = PlayerId::new();
        world.insert_operator(starter_operator(cy_id, Uuid::new_v4(), "cy", "Cy"));
        let cy = SessionIdentity::new(cy_id, "Cy", Permissions::player());
        let cy_session = cy.session_id;
        world.connect_player(cy).unwrap();
        for index in 0..INBOX_CAP {
            advance(&mut world);
            world.apply(ada, &format!("mail send bob letter {index}")).unwrap();
        }
        assert_eq!(world.mailbox.count_for(bob_id), INBOX_CAP);
        advance(&mut world);
        let err = world.apply(ada, "mail send bob one more").unwrap_err();
        assert!(err.to_string().contains("full"), "{err}");
        advance(&mut world);
        let err = world.apply(cy_session, "mail send bob from cy").unwrap_err();
        assert!(err.to_string().contains("full"), "{err}");
        assert_eq!(world.mailbox.count_for(bob_id), INBOX_CAP);
        assert!(world
            .mailbox
            .inbox(bob_id)
            .iter()
            .all(|message| message.body.starts_with("letter")));

        advance(&mut world);
        world.apply(ada, "mail send cy first").unwrap();
        let err = world.apply(ada, "mail send cy second").unwrap_err();
        assert!(err.to_string().contains("Wait a moment"), "{err}");
        assert_eq!(world.mailbox.count_for(cy_id), 1);
        advance(&mut world);
        world.apply(ada, "mail send cy second").unwrap();
        assert_eq!(world.mailbox.count_for(cy_id), 2);
    }

    #[test]
    fn either_ignore_refuses_mail_without_naming_the_side() {
        let (mut world, ada, bob, _, bob_id) = pair();
        world.apply(bob, "ignore ada").unwrap();
        let err = world.apply(ada, "mail send bob secret-from-ada").unwrap_err();
        assert!(err.to_string().contains("ignore/block"), "{err}");
        assert!(!err.to_string().contains("secret-from-ada"));
        assert!(world.mailbox.count_for(bob_id) == 0);
        let err = world.apply(bob, "mail send ada secret-from-bob").unwrap_err();
        assert!(err.to_string().contains("ignore/block"), "{err}");
        world.apply(bob, "unignore ada").unwrap();
        world.apply(ada, "ignore bob").unwrap();
        let err = world.apply(ada, "mail send bob after-ada-ignores").unwrap_err();
        assert!(err.to_string().contains("ignore/block"), "{err}");
        assert!(world.mailbox.messages.iter().all(|message| !message.body.contains("secret")));
        world.apply(ada, "unignore bob").unwrap();
        world.apply(ada, "mail send bob after the ignore").unwrap();
        assert_eq!(world.mailbox.count_for(bob_id), 1);
        world.apply(ada, "whisper bob hi").unwrap();
        assert!(feed(&world, bob).contains("hi"));
    }

    #[test]
    fn reload_keeps_read_state_and_does_not_reuse_a_deleted_id() {
        let (mut world, ada, bob, ada_id, bob_id) = pair();
        world.detach(bob).unwrap();
        world.apply(ada, "mail send bob keep this").unwrap();
        advance(&mut world);
        world.apply(ada, "mail send bob drop this").unwrap();
        let bob = SessionIdentity::new(bob_id, "Robert", Permissions::player());
        let bob_session = bob.session_id;
        world.connect_player(bob).unwrap();
        world.apply(bob_session, "mail read 1").unwrap();
        world.apply(bob_session, "mail delete 2").unwrap();
        let stored = Mailbox::from_stored(world.mailbox.messages.clone(), world.mailbox.next_id);
        let mut again = WorldState::for_tests();
        again.insert_operator(starter_operator(ada_id, Uuid::new_v4(), "ada", "Ada"));
        again.insert_operator(starter_operator(bob_id, Uuid::new_v4(), "bob", "Robert"));
        again.install_mail(stored);
        let bob = SessionIdentity::new(bob_id, "Robert", Permissions::player());
        let bob_session = bob.session_id;
        again.connect_player(bob).unwrap();
        let ada = SessionIdentity::new(ada_id, "Ada", Permissions::player());
        let ada_session = ada.session_id;
        again.connect_player(ada).unwrap();
        assert!(again.mailbox.owned(bob_id, 1).unwrap().read_at.is_some());
        assert!(again.mailbox.owned(bob_id, 2).is_none());
        again.apply(bob_session, "mail read 1").unwrap();
        assert!(overlay(&again, bob_session).contains("Status: Read"));
        assert!(overlay(&again, bob_session).contains("keep this"));
        advance(&mut again);
        again.apply(ada_session, "mail send bob a new letter").unwrap();
        assert!(again.mailbox.owned(bob_id, 2).is_none());
        assert_eq!(again.mailbox.inbox(bob_id)[0].id, 3);
        assert_eq!(again.mailbox.inbox(bob_id)[0].body, "a new letter");
    }

    #[test]
    fn spectators_can_read_and_cannot_send_or_delete() {
        let (mut world, ada, bob, _, bob_id) = pair();
        world.apply(ada, "mail send bob for the spectator").unwrap();
        world.detach(bob).unwrap();
        let bob = SessionIdentity::new(bob_id, "Robert", Permissions::spectator());
        let bob_session = bob.session_id;
        world.connect_player(bob).unwrap();
        world.apply(bob_session, "mail").unwrap();
        assert!(overlay(&world, bob_session).contains("1 unread of 1"));
        world.apply(bob_session, "mail read 1").unwrap();
        assert!(overlay(&world, bob_session).contains("for the spectator"));
        let err = world.apply(bob_session, "mail send ada hello").unwrap_err();
        assert!(err.to_string().contains("cannot speak"), "{err}");
        let err = world.apply(bob_session, "mail delete 1").unwrap_err();
        assert!(err.to_string().contains("cannot do that"), "{err}");
        assert!(world.mailbox.owned(bob_id, 1).is_some());
    }
}

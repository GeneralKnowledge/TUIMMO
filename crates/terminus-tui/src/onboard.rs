//! First-connection screens. Secrets stay in this form until the protocol call.
//! The world simulation never sees a password or a recovery code.

use terminus_core::Key;

#[derive(Clone, Debug)]
pub struct Onboard {
    pub gate: Gate,
    pub kind: String,
    from_session: bool,
}

#[derive(Clone, Debug)]
pub enum Gate {
    Welcome,
    Form(Form),
    Code(String),
    Account { message: String },
}

#[derive(Clone, Debug)]
pub struct Form {
    pub purpose: Purpose,
    pub fields: Vec<Field>,
    pub index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Guest,
    Login,
    Register,
    Recover,
    SetPassword,
    Upgrade,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub label: &'static str,
    pub value: String,
    pub secret: bool,
    pub optional: bool,
}

pub enum OnboardEffect {
    None,
    Quit,
    SubmitGuest {
        username: String,
        operator_name: String,
    },
    SubmitLogin {
        username: String,
        password: String,
    },
    SubmitRegister {
        username: String,
        password: String,
        operator_name: String,
        email: Option<String>,
    },
    SubmitRecover {
        username: String,
        recovery_code: String,
    },
    SubmitSecret {
        password: String,
        upgrade: bool,
        email: Option<String>,
    },
    Rotate,
    Revoke,
    EnterWorld,
}

impl Onboard {
    pub fn welcome() -> Self {
        Self { gate: Gate::Welcome, kind: String::new(), from_session: false }
    }

    pub fn code(code: String, kind: String) -> Self {
        Self { gate: Gate::Code(code), kind, from_session: true }
    }

    pub fn account(kind: String, message: String) -> Self {
        Self { gate: Gate::Account { message }, kind, from_session: true }
    }

    pub fn body(&self) -> String {
        match &self.gate {
            Gate::Welcome => "\
TERMINUS
THE UNDERNET
A world that continues without you.

Welcome, stranger. The name on your computer is not an account here.
No email is required.

[1] Enter as a guest
[2] Log in to an existing account
[3] Create a permanent account
[4] Recover an existing identity

Choose an option. Ctrl-C quits."
                .into(),
            Gate::Form(form) => form.help(),
            Gate::Code(code) => format!(
                "Save this recovery code somewhere safe.\n\n\
{code}\n\n\
It is the way back to this character if you disconnect or the server restarts.\n\
It will not be shown again. This terminal is not storing it.\n\n\
Enter continues into the world.\n\
p sets a password (a guest becomes a permanent account).\n\
Later, type account at the prompt to rotate the code."
            ),
            Gate::Account { message } => format!(
                "{message}\n\nkind: {}\n\n\
[1] Rotate recovery code\n\
[2] {}\n\
[3] Revoke recovery code\n\
Esc returns to the world.",
                if self.kind.is_empty() { "…" } else { &self.kind },
                if self.kind == "guest" {
                    "Upgrade to a permanent account"
                } else {
                    "Change password"
                },
            ),
        }
    }

    pub fn prompt(&self) -> (&'static str, String, bool) {
        match &self.gate {
            Gate::Welcome => ("choose> ", String::new(), false),
            Gate::Code(_) => ("enter> ", String::new(), false),
            Gate::Account { .. } => ("choose> ", String::new(), false),
            Gate::Form(form) => {
                let field = form.current();
                (field.label, field.value.clone(), field.secret)
            }
        }
    }

    pub fn handle(&mut self, key: Key) -> OnboardEffect {
        if key == Key::CtrlC {
            return OnboardEffect::Quit;
        }
        match &mut self.gate {
            Gate::Welcome => self.welcome_key(key),
            Gate::Form(_) => self.form_key(key),
            Gate::Code(_) => match key {
                Key::Enter => OnboardEffect::EnterWorld,
                Key::Char('p') | Key::Char('P') => {
                    let upgrade = self.kind == "guest";
                    self.gate = Gate::Form(Form::password(upgrade));
                    OnboardEffect::None
                }
                _ => OnboardEffect::None,
            },
            Gate::Account { .. } => match key {
                Key::Esc => OnboardEffect::EnterWorld,
                Key::Char('1') => OnboardEffect::Rotate,
                Key::Char('2') => {
                    let upgrade = self.kind == "guest";
                    self.gate = Gate::Form(Form::password(upgrade));
                    OnboardEffect::None
                }
                Key::Char('3') => OnboardEffect::Revoke,
                _ => OnboardEffect::None,
            },
        }
    }

    fn welcome_key(&mut self, key: Key) -> OnboardEffect {
        let purpose = match key {
            Key::Char('1') => Some(Purpose::Guest),
            Key::Char('2') => Some(Purpose::Login),
            Key::Char('3') => Some(Purpose::Register),
            Key::Char('4') => Some(Purpose::Recover),
            _ => None,
        };
        if let Some(purpose) = purpose {
            self.gate = Gate::Form(Form::new(purpose));
        }
        OnboardEffect::None
    }

    fn form_key(&mut self, key: Key) -> OnboardEffect {
        match key {
            Key::Esc => {
                if self.from_session {
                    return OnboardEffect::EnterWorld;
                }
                self.gate = Gate::Welcome;
                return OnboardEffect::None;
            }
            Key::Tab => {
                if let Gate::Form(form) = &mut self.gate {
                    form.index = (form.index + 1) % form.fields.len().max(1);
                }
                return OnboardEffect::None;
            }
            Key::BackTab => {
                if let Gate::Form(form) = &mut self.gate {
                    let len = form.fields.len().max(1);
                    form.index = (form.index + len - 1) % len;
                }
                return OnboardEffect::None;
            }
            Key::Enter => return self.submit(),
            _ => {}
        }
        let Gate::Form(form) = &mut self.gate else {
            return OnboardEffect::None;
        };
        let Some(field) = form.fields.get_mut(form.index) else {
            return OnboardEffect::None;
        };
        match key {
            Key::Backspace => {
                field.value.pop();
            }
            Key::CtrlU => field.value.clear(),
            Key::Char(ch) if !ch.is_control() => field.value.push(ch),
            _ => {}
        }
        OnboardEffect::None
    }

    fn submit(&mut self) -> OnboardEffect {
        let Gate::Form(form) = &self.gate else {
            return OnboardEffect::None;
        };
        match form.purpose {
            Purpose::Guest => {
                let username = form.value("user> ");
                let operator_name = form.value("name> ");
                if username.is_empty() || operator_name.is_empty() {
                    return OnboardEffect::None;
                }
                OnboardEffect::SubmitGuest { username, operator_name }
            }
            Purpose::Login => {
                let username = form.value("user> ");
                let password = form.value("pass> ");
                if username.is_empty() || password.is_empty() {
                    return OnboardEffect::None;
                }
                OnboardEffect::SubmitLogin { username, password }
            }
            Purpose::Register => {
                let username = form.value("user> ");
                let password = form.value("pass> ");
                let confirm = form.value("again> ");
                let operator_name = {
                    let name = form.value("name> ");
                    if name.is_empty() {
                        username.clone()
                    } else {
                        name
                    }
                };
                let email = {
                    let raw = form.value("email> ");
                    if raw.is_empty() {
                        None
                    } else {
                        Some(raw)
                    }
                };
                if username.is_empty() || password.is_empty() || password != confirm {
                    return OnboardEffect::None;
                }
                OnboardEffect::SubmitRegister { username, password, operator_name, email }
            }
            Purpose::Recover => {
                let username = form.value("user> ");
                let recovery_code = form.value("code> ");
                if username.is_empty() || recovery_code.is_empty() {
                    return OnboardEffect::None;
                }
                OnboardEffect::SubmitRecover { username, recovery_code }
            }
            Purpose::SetPassword | Purpose::Upgrade => {
                let password = form.value("pass> ");
                let confirm = form.value("again> ");
                let email = {
                    let raw = form.value("email> ");
                    if raw.is_empty() {
                        None
                    } else {
                        Some(raw)
                    }
                };
                if password.is_empty() || password != confirm {
                    return OnboardEffect::None;
                }
                OnboardEffect::SubmitSecret {
                    password,
                    upgrade: form.purpose == Purpose::Upgrade,
                    email,
                }
            }
        }
    }
}

impl Form {
    fn new(purpose: Purpose) -> Self {
        let fields = match purpose {
            Purpose::Guest => vec![field("user> ", false, false), field("name> ", false, false)],
            Purpose::Login => vec![field("user> ", false, false), field("pass> ", true, false)],
            Purpose::Register => vec![
                field("user> ", false, false),
                field("pass> ", true, false),
                field("again> ", true, false),
                field("name> ", false, false),
                field("email> ", false, true),
            ],
            Purpose::Recover => vec![field("user> ", false, false), field("code> ", true, false)],
            Purpose::SetPassword => {
                vec![field("pass> ", true, false), field("again> ", true, false)]
            }
            Purpose::Upgrade => vec![
                field("pass> ", true, false),
                field("again> ", true, false),
                field("email> ", false, true),
            ],
        };
        Self { purpose, fields, index: 0 }
    }

    fn password(upgrade: bool) -> Self {
        Self::new(if upgrade { Purpose::Upgrade } else { Purpose::SetPassword })
    }

    fn current(&self) -> &Field {
        &self.fields[self.index]
    }

    fn value(&self, label: &str) -> String {
        self.fields
            .iter()
            .find(|field| field.label == label)
            .map(|field| field.value.trim().to_string())
            .unwrap_or_default()
    }

    fn help(&self) -> String {
        let intro = match self.purpose {
            Purpose::Guest => {
                "A guest needs no email and no password.\n\
The recovery code on the next screen is how you return.\n\
Handle: a letter, then letters, digits, or _. 3–16 characters.\n\
Operator name: what other people see. 2–24 characters."
            }
            Purpose::Login => "Log in with the account handle and password. Passwords are hidden.",
            Purpose::Register => {
                "A permanent account needs a handle and a password. Email is optional.\n\
Leave email blank. Passwords are 8–128 characters and are not shown."
            }
            Purpose::Recover => {
                "Type the handle and the recovery code you saved.\n\
A wrong code does not say whether the handle exists."
            }
            Purpose::SetPassword => "Choose a new password. It will not be echoed.",
            Purpose::Upgrade => {
                "This keeps the same character. Set a password. Email is optional.\n\
The recovery code you already saved still works until you rotate it."
            }
        };
        let lines = self
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                let mark = if index == self.index { ">" } else { " " };
                let optional = if field.optional { " (optional)" } else { "" };
                format!("{mark} {}{optional}", field.label.trim())
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("{intro}\n\n{lines}\n\nTab switches fields. Enter submits. Esc goes back.")
    }
}

fn field(label: &'static str, secret: bool, optional: bool) -> Field {
    Field { label, value: String::new(), secret, optional }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_opens_guest_form() {
        let mut onboard = Onboard::welcome();
        assert!(matches!(onboard.handle(Key::Char('1')), OnboardEffect::None));
        assert!(matches!(onboard.gate, Gate::Form(Form { purpose: Purpose::Guest, .. })));
        assert!(onboard.body().contains("no email"));
    }

    #[test]
    fn password_field_is_marked_secret() {
        let mut onboard = Onboard::welcome();
        onboard.handle(Key::Char('2'));
        onboard.handle(Key::Char('a'));
        onboard.handle(Key::Tab);
        onboard.handle(Key::Char('s'));
        onboard.handle(Key::Char('e'));
        onboard.handle(Key::Char('c'));
        onboard.handle(Key::Char('r'));
        onboard.handle(Key::Char('e'));
        onboard.handle(Key::Char('t'));
        let (_label, value, secret) = onboard.prompt();
        assert!(secret);
        assert_eq!(value, "secret");
        assert!(!onboard.body().contains("secret"));
    }

    #[test]
    fn mismatched_confirmation_does_not_submit() {
        let mut onboard = Onboard::welcome();
        onboard.handle(Key::Char('3'));
        type_field(&mut onboard, "ada");
        onboard.handle(Key::Tab);
        type_field(&mut onboard, "long-enough");
        onboard.handle(Key::Tab);
        type_field(&mut onboard, "different1");
        onboard.handle(Key::Tab);
        type_field(&mut onboard, "Ada");
        assert!(matches!(onboard.handle(Key::Enter), OnboardEffect::None));
    }

    fn type_field(onboard: &mut Onboard, text: &str) {
        for ch in text.chars() {
            onboard.handle(Key::Char(ch));
        }
    }
}

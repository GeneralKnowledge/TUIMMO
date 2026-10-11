# Other locations

These seven rooms are implemented and part of the graph. They do not have concept art or floorplans yet. Use them as continuity checks when drawing the shop, the alley, or the relay. Anything under **Proposed** is not in the game.

Wall-by-wall layouts below are **interpretation**, only enough to stop the priority pictures from inventing doors.

## roof-array — Roof Array

- **Type:** Roof of the coffee shop. **Access:** public, only from the shop.
- **Implemented description:** Antennae and a folding chair. The city is a circuit of wet light. The stair down smells like coffee.
- **Exit:** `down` → `coffee-shop`. No path to the relay.
- **Object:** `antenna` (`array`, `mast`). The mast is lashed with tape and points at the old relay, a few blocks of rooftops away.
- **Check:** the shop stair comes up here. Do not draw this roof on the exchange or the relay.
- **Proposed:** none.

## exchange — The Exchange

- **Type:** Half-dark trading floor, street level off the alley. **Access:** public.
- **Implemented description:** Screens still scroll prices nobody can settle. People come for rumours, not fills.
- **Exits:** `alley` → `neon-alley`, `market` → `stack-market`, `service` → `service-corridor`.
- **Object:** `ticker` (`screens`). Static text: `RELAY UNSTAFFED. ARCHIVE LOCKED.` It does not change when the archive opens.
- **Check:** the alley's east end is this room's alley door. The relay is not visible as a neighbouring stall.
- **Proposed:** make the ticker mention the archive only while it is sealed.

## stack-market — Stack Market

- **Type:** Stalls of server racks beside the exchange. **Access:** public.
- **Implemented description:** Cables, batteries, and stories. Awnings are foil blankets.
- **Exits:** `exchange` → `exchange`, `service` → `service-corridor`.
- **Object:** `stall` (`rack`). Blank plates for sale. The vendor is not here. Sign copy: `CASH ONLY, THEN LEAVE.` No purchase command exists.
- **Check:** foil and racks belong here, not in the coffee shop.
- **Proposed:** a vendor or a trade. Not implemented. There is no currency.

## service-corridor — Service Corridor

- **Type:** Maintenance hall under the exchange. **Access:** unlocked. Reads as staff space.
- **Implemented description:** Cable trays overhead. One hatch in the floor was forced and then abandoned.
- **Exits:** `exchange` → `exchange`, `market` → `stack-market`, `down` → `abandoned-exchange`.
- **Object:** `panel` (`hatch`). The panel lists floors. The lowest entry is crossed out in marker. Inspecting it grants nothing. The `down` exit is separate and always open.
- **Check:** this is the vertical link the alley does not have. The alley goes down only into the tunnels.
- **Proposed:** none.

## abandoned-exchange — The Abandoned Exchange

- **Type:** Lower floor of the exchange. **Access:** public, via service `down` or relay `floor`.
- **Implemented description:** Desks shoved aside, ash-coloured carpet, one terminal with power.
- **Exits:** `up` → `service-corridor`, `relay` → `old-relay`.
- **Object:** `terminal` (`screen`, `computer`). Amber text on a login nobody will answer. The local log is mounted.
- **Discovery:** `inspect terminal` grants flag `read-terminal` and item `torn-log` ("A scrap of maintenance log. The last line is just a room number."). First time, the terminal coughs up one page and you tear it free. There is no room-number puzzle after that.
- **Check:** the relay's `floor` passage arrives here. It must not be drawn as the archive.
- **Proposed:** using the room number on the torn log. Not implemented.

## cooling-tunnels — Cooling Tunnels

- **Type:** Round concrete duct. **Access:** public.
- **Implemented description:** Constant fan hum. Condensate drips in time. The tunnel runs from the alley toward an old communications node.
- **Exits:** `up` → `neon-alley`, `relay` → `old-relay`. No exit to the abandoned exchange.
- **Object:** `grate` (`fan`). The grate is warm. Air pushes up from somewhere that still has electricity.
- **Check:** the alley's `down` is this tunnel's `up`. The far end is the relay's `tunnels` door. Do not paint a junction into the archive.
- **Proposed:** none. Length is not simulated.

## black-archive — The Black Archive

- **Type:** Sealed stacks. **Access:** restricted. Hidden from the exit list until `cipher-plate`. Enter from the relay with `go archive`. Leave with `relay`.
- **Implemented description:** Quiet, dry, lights low. You should not hear the city from here, but you can.
- **Object:** `stacks` (`shelves`, `archive`). Labelled with route numbers, not titles. Copy says one gap matches the plate you carried in. That sentence is static: it does not check the inventory.
- **Check:** one room, one door back to the relay. Dry air is the contrast with the tunnels and the alley. Hearing the city is text, not a window exit.
- **Proposed:** state-dependent stack copy, and anything that would consume or read the plate. The plate is not spent on entry.

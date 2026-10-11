# Region: Undernet

One region. There are no other regions in the build. Future districts should be new location IDs plus exits, not a second map server.

## Intent

A player should feel the shop as a place to come back to, the alley as a real street edge, and the relay as a machine room that still has one loose part. The exchange block is the social and rumour layer between those. The archive is a small reward for reading the room, not a dungeon.

## Landmark roles

| ID | Role in the region |
| --- | --- |
| `coffee-shop` | Social hub and spawn. |
| `roof-array` | View. Confirms the relay exists in the city without joining it to the shop. |
| `neon-alley` | The only street the player walks. Orients shop, exchange, and the tunnel mouth. |
| `exchange` | Public rumour floor. The ticker mentions the unstaffed relay and the locked archive. |
| `stack-market` | Scrap trade as scenery. No economy is implemented. |
| `service-corridor` | The building's maintenance layer. Points down. |
| `abandoned-exchange` | First discovery: the torn log. |
| `cooling-tunnels` | The underground shortcut toward the relay. |
| `old-relay` | Second discovery: the cipher plate. Gate to the archive. |
| `black-archive` | Quiet end room. No further implemented content. |

## Atmosphere by depth

Street level (shop, alley, exchange, market, roof) is wet, amber, and occupied. Below that (service, tunnels, abandoned floor, relay) the palette cools and the clutter turns into infrastructure. The archive breaks the pattern on purpose: dry, low, kept.

## Boundaries

Players cannot leave this set of rooms. There is no street door out of the shop, no exit from the roof except `down`, and no exit from the archive except `relay`. Global chat is heard everywhere, including rooms the speaker cannot see. That is social range, not a physical sightline.

## Continuity for art

Priority pictures are the shop, the alley, and the relay. The other rooms are specified in `locations/continuity.md` so those three cannot grow doors that the rest of the graph does not have.

When a later location gets a full reference, follow `style-guide.md` and add its id to `implemented.graph` only if the code already has it. Proposals do not go in that file.

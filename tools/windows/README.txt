ROTFALL
=======
Written with AI. Free software under the GNU GPL version 3; see LICENSE.
Source code: https://github.com/1stwbsm/rotfall

Double-click rotfall.exe to play. Nothing needs installing.

It needs 64-bit Windows 10 or 11 and a graphics driver with Vulkan or OpenGL 4.3.
If it won't start, updating your graphics driver is the first thing to try.

If Windows SmartScreen warns about an unrecognized app, that is because this build is not
code-signed: choose "More info", then "Run anyway".

CONTROLS
  WASD / mouse       Move / aim
  Left mouse         Shoot
  R                  Reload
  Shift / Space      Sprint / dodge roll
  G                  Grenade at the cursor
  1-9, wheel, Q/E    Switch weapon
  F                  Flashlight
  Tab                Armory: guns, ammo, grenades
  B                  Build mode (drag to lay walls and traps)
  U / T / X          Upgrade / repair / sell what the cursor is on
  Enter              Start the next wave early
  F5 / F9            Quicksave / quickload
  P                  Player list and server panel (multiplayer)
  F11                Fullscreen
  Esc                Pause

MULTIPLAYER
  Host from the menu (Multiplayer, Host a server), or run start-server.bat for a dedicated
  server. Windows Firewall will ask the first time: allow it, or friends cannot connect.
  For internet play, the host also forwards TCP port 27416 on their router.
  Play with people you trust: the server password is not encrypted.

FILES
  Saves, settings and records are in %APPDATA%\rotfall
  If the game crashes, details are written to crash.log in the same folder.

TROUBLESHOOTING
  The game uses Vulkan, falling back to OpenGL. If it looks wrong or crashes, try OpenGL
  directly. In a command prompt:
      set WGPU_BACKEND=gl
      rotfall.exe
  Low frame rate: run   rotfall.exe --scale 0.7

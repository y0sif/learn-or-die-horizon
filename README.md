# Learn or Die (Horizon port)

Learn or Die, ported to [Horizon](https://github.com/horizon-rust/horizon), the Rust rewrite of Minecraft
Java 26.3: a German teacher narrates what you do and quizzes you on it.

## What it does

The port has the core of Learn or Die with the German pack:

- The teacher says out loud, in German, what you do: mining, building, picking up and dropping items, moving,
  fighting, eating, the weather and more. Riding and fishing are said in single player and for the host of a
  LAN world. The sentence shows on screen with a translation, and his portrait talks along.
- Questions come up as you play: pick the German for the word or sentence shown, or for the phrase you hear,
  with keys 1 to 4 or the mouse, or type it. The game pauses in single player while a question is open.
- Right answers bring buffs or friendly mobs. Wrong ones bring debuffs, five monsters, lost hearts or death, as
  you choose, and always death in a Hardcore world. After several wrong answers in a row the angry teacher drops
  your inventory. Rewards and penalties work in single player, and for the host of a LAN world.
- Every 20 minutes of play an exam asks up to 20 phrases you have met and shows your score.
- The dictionary lists the phrases you know, word by word, with grammar notes and other forms, and plays them
  normal or slow. Item tooltips show the German noun, and "what is this" names the block or mob you look at.
- Your answers are saved per world. A phrase you know is not asked or said again in that world.

A short guide opens in your first world. Not ported: voice answers, and the HUD's score counter, learned-phrase
banner and jingle.

## Keys

| Key | Action |
|---|---|
| N | Turn the teacher on or off |
| M | Mute or unmute the teacher's voice |
| K | Switch between normal and slow speech |
| I | Open the settings |
| J | Open the dictionary |
| V | Say what you are looking at |
| U | Open the guide |
| 1 to 4 | Answer a question |
| Enter | Check a typed answer |
| Esc | Pause during a question; it comes back when you resume |

These keys can be changed in Controls, Key Binds, except 1 to 4, Enter and Esc. With a separate stopgap mod, a
controller works on the teacher's screens: A to press, the right stick to scroll, B to go back, the on-screen
keyboard to type, and the D-pad moves a slider after A grabs it.

## Install

You need Horizon with game build `8f83e5c` (nightly 2026-11-21) and the official Learn or Die German 0.6.2 jar
for Minecraft 26.3, `learn-or-die-german-0.6.2+mc26.3.jar`, from the
[official release](https://github.com/Larenlearn/learn-or-die/releases/tag/v0.6.2). The port reads the
German pack, the voices and the portrait from that jar. It contains none of them itself.

1. Build the mod into an instance:
   `horizon-launcher native-build <this folder> --instance "<your instance>" --trust-native-code`.
2. Start the instance. Without the jar, the launcher stops and names the path it expects. Copy the unmodified
   jar to that path, then start again.

## Settings

Press I in a world to change the settings: question type and frequency, penalties and rewards, the angry
teacher, the exam, speech speed and volume, tooltips, and which actions the teacher talks about. They are saved
at once to `config/learnordie.json` in the instance folder, with the original's setting names and defaults.

## Credits and support

Learn or Die is made by Laren. Website: [learnordie.me](https://learnordie.me). Official mod:
[github.com/Larenlearn/learn-or-die](https://github.com/Larenlearn/learn-or-die). This port is made with
Laren's [permission](https://github.com/Larenlearn/learn-or-die/issues/17).

For problems with the port, open an issue here. For everything else, see the official mod.

## License

All Rights Reserved. Learn or Die's code, voices, texts and word lists belong to Laren, and this port is under
the same terms. See [LICENSE](LICENSE).

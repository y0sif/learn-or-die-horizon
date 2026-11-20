# Learn or Die (Horizon port)

An unofficial Horizon port of Learn or Die by Laren: a German teacher narrates what you do and quizzes you on it.

Learn or Die is made by Laren. Website: [learnordie.me](https://learnordie.me). Official mod:
[github.com/Larenlearn/learn-or-die](https://github.com/Larenlearn/learn-or-die).

This port runs it natively on [Horizon](https://github.com/horizon-rust/horizon), the Rust rewrite of
Minecraft Java 26.3, with Laren's [permission](https://github.com/Larenlearn/learn-or-die/issues/17). For
problems with the port, open an issue here. For everything else, see the official mod.

## What it does

The port has the core of Learn or Die with the German pack:

- The teacher says out loud, in German, what you do: breaking and placing blocks, picking up items and holding
  an item. The sentence shows on screen with a translation, and his portrait talks along.
- Multiple-choice questions come up as you play: pick the German for the word or sentence shown, or for the
  phrase you hear, with keys 1 to 4 or the mouse. The game pauses in single player while a question is open.
- Your answers are saved per world. A phrase you know is not asked or said again in that world.

Not ported yet: rewards and penalties, typed answers, the exam, the dictionary, "what is this", item tooltips,
the settings screens and voice answers.

## Keys

| Key | Action |
|---|---|
| N | Turn the teacher on or off |
| M | Mute or unmute the teacher's voice |
| 1 to 4 | Answer a question |
| Esc | Pause during a question; it comes back when you resume |

Both N and M can be changed in Controls, Key Binds.

## Install

You need Horizon with game build `0b14f8a` (nightly 2026-11-18) and the official Learn or Die German 0.6.2 jar
for Minecraft 26.3, `learn-or-die-german-0.6.2+mc26.3.jar`, from the
[official release](https://github.com/Larenlearn/learn-or-die/releases/tag/v0.6.2). The port reads the
German pack, the voices and the portrait from that jar. It contains none of them itself.

1. Build the mod into an instance:
   `horizon-launcher native-build <this folder> --instance "<your instance>" --trust-native-code`.
2. Start the instance. Without the jar, the launcher stops and names the path it expects. Copy the unmodified
   jar to that path, then start again.

## Settings

The settings are in `config/learnordie.json` in the instance folder. They use the original's setting names and
defaults. The file is written on the first start.

## License

All Rights Reserved. Learn or Die's code, voices, texts and word lists belong to Laren, and this port is under
the same terms. See [LICENSE](LICENSE).

//! The teacher's voice: the core's sound commands played as local sounds on the voice source, at
//! the listener (the original's `SimpleSoundInstance` with no attenuation, relative), and the
//! quiz's ding. How long a clip lasts is the core's business (its lipsync length): with no audio
//! output the engine reports every sound stopped at once.
use super::client::Client;
use crate::core::SoundCommand;
use mod_api::{ClientHost, SoundPlay, SoundRef, SoundSource};

/// The question's ding: `forUI(EXPERIENCE_ORB_PICKUP, 1.3F, 0.8F)`.
pub(crate) const DING: &str = "minecraft:entity.experience_orb.pickup";
pub(crate) const CLICK: &str = "minecraft:ui.button.click";

pub(crate) fn ding(host: &dyn ClientHost, io: &dyn Client) {
    io.play(host, &SoundPlay::ui(DING).with_pitch(1.3).with_volume(0.8));
}

pub(crate) fn click(host: &dyn ClientHost, io: &dyn Client) {
    io.play(host, &SoundPlay::ui(CLICK));
}

/// Runs the speaker's commands. `playing` is the teacher's last sound, whose state the speaker is
/// told each tick.
pub(crate) fn run(
    host: &dyn ClientHost,
    io: &dyn Client,
    commands: Vec<SoundCommand>,
    playing: &mut Option<SoundRef>,
) {
    for command in commands {
        match command {
            SoundCommand::Play { event, volume, .. } => {
                let mut sound = SoundPlay::ui(&event).with_volume(volume);
                sound.source = SoundSource::Voice;
                *playing = io.play(host, &sound);
            }
            SoundCommand::Stop => {
                if let Some(sound) = playing.take() {
                    io.stop(host, sound);
                }
            }
        }
    }
}

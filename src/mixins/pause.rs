fn quiz_pause(value: bool, game: &Game<'_>) -> bool {
    for (warning, text) in mod_learnordie::game::take_log_lines() {
        if warning {
            ::log::warn!(target: "learnordie", "{text}");
        } else {
            ::log::info!(target: "learnordie", "{text}");
        }
    }
    let shared = game.is_published() || game.has_split_players();
    mod_learnordie::game::set_shared(shared);
    value
        || (mod_learnordie::game::quiz_pauses()
            && game.screen == Screen::Mod
            && game.has_singleplayer_server()
            && !shared)
}

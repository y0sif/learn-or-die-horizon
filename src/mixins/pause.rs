fn quiz_pause(value: bool, game: &Game<'_>) -> bool {
    value
        || (mod_learnordie::game::quiz_pauses()
            && game.screen == Screen::Mod
            && game.has_singleplayer_server()
            && !game.is_published())
}

fn lod_pad_typing_screen(value: bool, game: &Game<'_>) -> bool {
    value || mod_learnordie::game::pad_typing(game.mod_screen())
}

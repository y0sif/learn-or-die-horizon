fn lod_pad_escape(game: &mut Game<'_>) -> ::core::ops::ControlFlow<Result<()>> {
    if !game.prompt_open()
        && game.mod_screen() == Some(mod_learnordie::game::MOD_ID)
        && game.screen_key_pressed(KeyCode::Escape, false)
    {
        return ::core::ops::ControlFlow::Break(Ok(()));
    }
    ::core::ops::ControlFlow::Continue(())
}

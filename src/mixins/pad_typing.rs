fn lod_pad_typing(game: &mut Game<'_>, out: &osk::Output) -> ::core::ops::ControlFlow<()> {
    if game.dialog_open() || !mod_learnordie::game::pad_typing(game.mod_screen()) {
        return ::core::ops::ControlFlow::Continue(());
    }
    match out {
        osk::Output::Text(text) => game.screen_text_each(text, |_, _| {}),
        osk::Output::Backspace => {
            game.screen_key_pressed(KeyCode::Backspace, false);
        }
        osk::Output::Enter | osk::Output::Done => {
            game.screen_key_pressed(KeyCode::Enter, false);
            game.pad.menu.osk = None;
        }
        _ => return ::core::ops::ControlFlow::Continue(()),
    }
    ::core::ops::ControlFlow::Break(())
}

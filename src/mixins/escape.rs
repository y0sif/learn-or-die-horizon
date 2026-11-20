fn quiz_escape(value: (), game: &mut Game<'_>) {
    if mod_learnordie::game::take_escape_pause() && game.screen == Screen::Playing {
        game.pause_focus = None;
        game.set_screen(Screen::Paused);
        game.pause_menu = true;
        game.capture(false);
    }
    value
}

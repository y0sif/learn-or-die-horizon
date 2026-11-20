fn lod_pad_sliders(value: (), game: &Game<'_>) {
    if game.mod_screen() == Some(mod_learnordie::game::MOD_ID) {
        for (rect, at, track) in mod_learnordie::game::pad_sliders() {
            let kind = horizon_client::widgets::Kind::Slider { value: at, track };
            horizon_client::widgets::target_as(rect, kind);
        }
    }
    value
}

//! Learn or Die (Horizon port): the game-independent teacher (`core`) wired to the game's hooks
//! (`game`). Ported from Learn or Die by Laren, All Rights Reserved, with permission.
//!
//! The teacher lives on the client, as in the original: detection, her voice, the HUD and the
//! question screen all run there, so she works on any server. The integrated server's half only
//! hands the client the world seed for the progress file's world key.
use mod_api::{
    AddClientReloadListenersEvent, ClientHost, ClientPlayerLoggingOutEvent, ClientTickPostEvent,
    Flow, GuiLayerDrawEvent, Hooks, HostSide, LogicalSide, RegisterGuiLayersEvent, Registrar,
    RegistrationContext, RightClickBlockEvent, ScreenKeyPressedPreEvent,
    ScreenMouseButtonPressedPreEvent, ScreenRenderPostEvent, ServerHost, ServerStartedEvent,
    ServerStoppedEvent,
};

pub mod core;
pub mod game;

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        game::keys::register(r);
    }

    fn register_gui_layers(
        &self,
        _cx: &RegistrationContext,
        event: &mut RegisterGuiLayersEvent<'_>,
    ) {
        game::hud::register(event);
    }

    fn add_client_reload_listeners(
        &self,
        _host: &dyn ClientHost,
        event: &mut AddClientReloadListenersEvent<'_>,
    ) {
        game::loading::add_listener(event.listeners);
    }

    fn server_started(&self, host: &mut dyn ServerHost, event: &mut ServerStartedEvent) {
        game::world::server_started(host, event.dedicated);
    }

    fn server_stopped(&self, _host: &mut dyn ServerHost, event: &mut ServerStoppedEvent) {
        if !event.dedicated {
            game::world::server_stopped();
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        game::client_tick(host);
    }

    fn client_player_logging_out(
        &self,
        host: &mut dyn ClientHost,
        _event: &mut ClientPlayerLoggingOutEvent,
    ) {
        game::logging_out(host);
    }

    /// The client's half of a right click on a block: the place check waits for the block.
    fn right_click_block(&self, host: HostSide, event: &mut RightClickBlockEvent<'_>) -> Flow {
        if let HostSide::Client(host) = host
            && event.click.side == LogicalSide::Client
        {
            game::detect::right_click_block(&*host, event.click);
        }
        Flow::Continue
    }

    fn gui_layer_draw(&self, host: &dyn ClientHost, event: &mut GuiLayerDrawEvent<'_>) {
        game::hud::draw(host, event);
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        game::screen::render(host, event);
    }

    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        game::screen::mouse_pressed(host, event)
    }

    fn screen_key_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenKeyPressedPreEvent<'_>,
    ) -> Flow {
        game::screen::key_pressed(host, event)
    }
}

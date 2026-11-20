//! Learn or Die (Horizon port): the game-independent teacher (`core`) wired to the game's hooks
//! (`game`). Ported from Learn or Die by Laren, All Rights Reserved, with permission.
//!
//! The teacher lives on the client, as in the original: detection, his voice, the HUD and the
//! question screen all run there, so he works on any server. The integrated server's half hands
//! the client the world seed for the progress file's world key, and applies an answer's
//! consequences to the host's own player (`game::server`).
//!
//! The client hooks act only for this window's own game: a split-screen player's game, in this
//! process too, would otherwise drive the same teacher with its own player (`game::hosting`).
use mod_api::{
    AddClientReloadListenersEvent, ClientAttackTargetEvent, ClientHost,
    ClientPlayerLoggingOutEvent, ClientTickPostEvent, EntityInteractEvent, Flow, GuiDraw,
    GuiLayerDrawEvent, Hand, HealthHudEvent, Hooks, HostSide, InteractionKeyMappingTriggeredEvent,
    InteractionResult, LogicalSide, RegisterGuiLayersEvent, Registrar, RegistrationContext,
    RenderLivingPreEvent, RightClickBlockEvent, ScreenCharTypedPreEvent, ScreenKeyPressedPreEvent,
    ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent,
    ScreenMouseDraggedPreEvent, ScreenRenderPostEvent, ServerHost, ServerStartedEvent,
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
        game::payload::register(r);
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
        if game::hosting() {
            game::client_tick(host);
        }
    }

    fn client_player_logging_out(
        &self,
        host: &mut dyn ClientHost,
        _event: &mut ClientPlayerLoggingOutEvent,
    ) {
        if game::hosting() {
            game::logging_out(host);
        }
    }

    /// The attack key acts on the crosshair's target: a block that breaks at once is counted at
    /// the tick's end.
    fn interaction_key_mapping_triggered(
        &self,
        host: &mut dyn ClientHost,
        event: &mut InteractionKeyMappingTriggeredEvent,
    ) -> Flow {
        if game::hosting() {
            game::detect::attack(&*host, event.key);
        }
        Flow::Continue
    }

    /// The client's half of a right click on a block: the place check waits for the block.
    fn right_click_block(&self, host: HostSide, event: &mut RightClickBlockEvent<'_>) -> Flow {
        if let HostSide::Client(host) = host
            && event.click.side == LogicalSide::Client
            && game::hosting()
        {
            game::detect::right_click_block(&*host, event.click);
        }
        Flow::Continue
    }

    /// The attack key hits the crosshair's entity (the first target): `hit.<entity>`, and a
    /// living one is watched for the kill.
    fn client_attack_target(
        &self,
        host: &mut dyn ClientHost,
        event: &mut ClientAttackTargetEvent<'_>,
    ) -> Flow {
        if let Some(&target) = event.targets.first()
            && game::hosting()
        {
            game::detect::attacked(&*host, target);
        }
        Flow::Continue
    }

    /// The client's half of a right click on an entity, main hand: `feed.<entity>`.
    fn entity_interact(
        &self,
        host: HostSide,
        event: &mut EntityInteractEvent<'_>,
    ) -> Option<InteractionResult> {
        if let HostSide::Client(host) = host
            && event.hand == Hand::Main
            && game::hosting()
        {
            game::detect::entity_used(&*host, event.kind, event.stack);
        }
        None
    }

    /// Every living entity drawn: one hurt or dying is kept for the tick (a client reads no
    /// health of another entity). Gated on `hosting` inside, after the two comparisons that
    /// let almost every entity through untouched.
    fn render_living_pre(
        &self,
        _host: &dyn ClientHost,
        event: &mut RenderLivingPreEvent<'_>,
    ) -> Flow {
        game::detect::drawn(event.entity);
        Flow::Continue
    }

    /// The health bar's inputs tell whether the world is a Hardcore one. Horizon draws its own
    /// bar.
    fn health_hud(
        &self,
        _host: &mut dyn ClientHost,
        event: &mut HealthHudEvent<'_>,
    ) -> Option<Vec<GuiDraw>> {
        if game::hosting() {
            game::detect::set_hardcore(event.inputs.hardcore);
        }
        None
    }

    fn gui_layer_draw(&self, host: &dyn ClientHost, event: &mut GuiLayerDrawEvent<'_>) {
        if game::hosting() {
            game::hud::draw(host, event);
        }
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        if game::hosting() {
            game::screen::render(host, event);
        }
    }

    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        if !game::hosting() {
            return Flow::Continue;
        }
        game::screen::mouse_pressed(host, event)
    }

    fn screen_key_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenKeyPressedPreEvent<'_>,
    ) -> Flow {
        if !game::hosting() {
            return Flow::Continue;
        }
        game::screen::key_pressed(host, event)
    }

    fn screen_char_typed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenCharTypedPreEvent<'_>,
    ) -> Flow {
        if !game::hosting() {
            return Flow::Continue;
        }
        game::screen::char_typed(host, event)
    }

    fn screen_mouse_button_released_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> Flow {
        if !game::hosting() {
            return Flow::Continue;
        }
        game::screen::mouse_released(host, event)
    }

    fn screen_mouse_dragged_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseDraggedPreEvent<'_>,
    ) -> Flow {
        if !game::hosting() {
            return Flow::Continue;
        }
        game::screen::mouse_dragged(host, event)
    }
}

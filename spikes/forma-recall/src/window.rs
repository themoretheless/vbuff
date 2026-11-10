//! Minimal winit + softbuffer host, adapted from Forma's binding example.
//!
//! The host owns the keyboard: arrows, Enter and Escape are popup commands and
//! never reach the text field. Everything else is forwarded to the runtime.
use super::Spike;
use std::{num::NonZeroU32, sync::Arc, time::Instant};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, Ime, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId, WindowLevel},
};

struct App {
    spike: Spike,
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    cursor: (f32, f32),
    shift: bool,
    command: bool,
    composing: bool,
    last_frame: Instant,
}

impl App {
    fn draw(&mut self) -> Result<(), String> {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32() * 1000.;
        self.last_frame = now;
        self.spike.form.update(|runtime| runtime.tick(dt.min(100.)))?;
        let window = self.window.as_ref().unwrap();
        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        let surface = self.surface.as_mut().unwrap();
        surface.resize(width, height).map_err(|e| e.to_string())?;
        let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
        let runtime = self.spike.form.runtime()?;
        runtime.paint_native(
            &mut buffer,
            size.width,
            size.height,
            window.scale_factor() as f32,
        )?;
        buffer.present().map_err(|e| e.to_string())?;
        if runtime.is_animating() {
            window.request_redraw();
        }
        Ok(())
    }

    fn event(&mut self, event: WindowEvent, event_loop: &ActiveEventLoop) -> Result<(), String> {
        match event {
            WindowEvent::CloseRequested => {
                eprintln!("exit: close requested");
                event_loop.exit();
                return Ok(());
            }
            WindowEvent::RedrawRequested => return self.draw(),
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.window.as_ref().unwrap().scale_factor() as f32;
                self.cursor = (position.x as f32 / scale, position.y as f32 / scale);
                let (x, y) = self.cursor;
                self.spike.form.update(|r| r.pointer(x, y, 0))?;
            }
            WindowEvent::CursorLeft { .. } => {
                self.spike.form.update(|r| r.pointer(-1., -1., 3))?;
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.cursor;
                self.spike.form.update(|r| {
                    r.pointer(x, y, if state == ElementState::Pressed { 1 } else { 2 })
                })?;
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.shift = modifiers.state().shift_key();
                self.command = if cfg!(target_os = "macos") {
                    modifiers.state().super_key()
                } else {
                    modifiers.state().control_key()
                };
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                self.composing = !text.is_empty();
                self.spike.form.update(|r| r.text_preedit(&text))?;
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.composing = false;
                self.spike.form.update(|r| r.text_insert(&text))?;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                match event.logical_key {
                    Key::Named(NamedKey::ArrowDown) if pressed => self.spike.vm.move_selection(1),
                    Key::Named(NamedKey::ArrowUp) if pressed => self.spike.vm.move_selection(-1),
                    Key::Named(NamedKey::PageDown) if pressed => self.spike.vm.move_selection(10),
                    Key::Named(NamedKey::PageUp) if pressed => self.spike.vm.move_selection(-10),
                    Key::Named(NamedKey::Enter) if pressed && !event.repeat => {
                        if self.spike.vm.activate_selected() {
                            for action in self.spike.vm.actions.borrow().iter() {
                                println!("UiAction emitted: {action:?}");
                            }
                            event_loop.exit();
                            return Ok(());
                        }
                    }
                    Key::Named(NamedKey::Escape) if pressed && !event.repeat => {
                        if self.spike.vm.query.get().is_empty() {
                            eprintln!("exit: Escape with an empty query");
                            event_loop.exit();
                            return Ok(());
                        }
                        self.spike.vm.query.set(String::new());
                        self.spike.vm.refresh(true);
                    }
                    Key::Named(name) if pressed => {
                        let key = match name {
                            NamedKey::Backspace => "Backspace",
                            NamedKey::Delete => "Delete",
                            NamedKey::ArrowLeft => "ArrowLeft",
                            NamedKey::ArrowRight => "ArrowRight",
                            NamedKey::Home => "Home",
                            NamedKey::End => "End",
                            _ => "",
                        };
                        let (shift, command) = (self.shift, self.command);
                        self.spike.form.update(|r| {
                            if !key.is_empty() && r.text_editing() {
                                r.text_key(key, shift, command);
                            } else if name == NamedKey::Space && r.text_editing() {
                                r.text_insert(" ");
                            }
                        })?;
                    }
                    Key::Character(key) if pressed && !self.composing => {
                        let (shift, command) = (self.shift, self.command);
                        self.spike.form.update(|r| {
                            if command {
                                r.text_key(&key, shift, true);
                            } else if let Some(text) = event.text {
                                r.text_insert(&text);
                            }
                        })?;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        if let Some(window) = &self.window {
            window.set_ime_allowed(self.spike.form.runtime()?.text_editing());
            window.request_redraw();
        }
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let setup = || -> Result<_, String> {
            // The popup shape forma::native_window cannot express yet:
            // borderless, fixed size, above other windows.
            let window = Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("vbuff · Forma spike")
                            .with_inner_size(LogicalSize::new(900., 580.))
                            .with_resizable(false)
                            .with_decorations(false)
                            .with_window_level(WindowLevel::AlwaysOnTop),
                    )
                    .map_err(|e| e.to_string())?,
            );
            let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
            let surface =
                softbuffer::Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
            Ok((window, surface))
        };
        match setup() {
            Ok((window, surface)) => {
                let redraw = window.clone();
                self.spike.form.on_dirty(move || redraw.request_redraw());
                window.request_redraw();
                window.focus_window();
                self.window = Some(window);
                self.surface = Some(surface);
            }
            Err(error) => {
                eprintln!("{error}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if let Err(error) = self.event(event, event_loop) {
            eprintln!("{error}");
            event_loop.exit();
        }
    }
}

pub fn run(spike: Spike) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop
        .run_app(&mut App {
            spike,
            window: None,
            surface: None,
            cursor: (0., 0.),
            shift: false,
            command: false,
            composing: false,
            last_frame: Instant::now(),
        })
        .map_err(|e| e.to_string())
}

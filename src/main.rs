use std::rc::Rc;

use gleam::gl;
use gtk::gdk::{GLContext, GLAPI};
use gtk::glib::Propagation;
use gtk::{glib, Application, ApplicationWindow};
use gtk::{prelude::*, GLArea};
use servo::compositing::windowing::{EmbedderCoordinates, EmbedderMethods, WindowMethods};
use servo::compositing::CompositeTarget;
use servo::euclid::{Box2D, Scale, Size2D};
use servo::webrender_traits::rendering_context::{GLVersion, RenderingContext};
use servo::{EventLoopWaker, Servo, WebView, WebViewDelegate};
use url::Url;

const APP_ID: &str = "org.gtk_rs.HelloWorld2";

fn main() -> glib::ExitCode {
    // Load GL pointers from epoxy (GL context management library used by GTK).
    {
        #[cfg(target_os = "macos")]
        let library = unsafe { libloading::os::unix::Library::new("libepoxy.0.dylib") }.unwrap();
        #[cfg(all(unix, not(target_os = "macos")))]
        let library = unsafe { libloading::os::unix::Library::new("libepoxy.so.0") }.unwrap();
        #[cfg(windows)]
        let library = libloading::os::windows::Library::open_already_loaded("libepoxy-0.dll")
            .or_else(|_| libloading::os::windows::Library::open_already_loaded("epoxy-0.dll"))
            .unwrap();

        epoxy::load_with(|name| {
            unsafe { library.get::<_>(name.as_bytes()) }
                .map(|symbol| *symbol)
                .unwrap_or(std::ptr::null())
        });
    }

    // Create a new application
    let app = Application::builder().application_id(APP_ID).build();

    // Connect to "activate" signal of `app`
    app.connect_activate(build_ui);

    // Run the application
    app.run()
}

fn build_ui(app: &Application) {
    let gl_area = GLArea::builder()
        .has_depth_buffer(true)
        .has_stencil_buffer(true)
        .width_request(800)
        .height_request(600)
        .build();
    gl_area.connect_realize(|area| {
        let gl = if area.api().contains(GLAPI::GL) {
            unsafe { gl::GlFns::load_with(epoxy::get_proc_addr) }
        } else {
            unsafe { gl::GlesFns::load_with(epoxy::get_proc_addr) }
        };
        let rendering_context = Rc::new(GTKRenderingContext {
            gl_area: area.clone(),
            gl,
        });

        // TODO Create Servo GObject that can create GLArea context
        let servo = Servo::new(
            Default::default(),
            Default::default(),
            rendering_context.clone(),
            Box::new(EmbedderDelegate(Waker(()))),
            rendering_context.clone(),
            None,
            CompositeTarget::ContextFbo,
        );
        servo.setup_logging();
        let url = Url::parse("https://servo.org").unwrap();
        let webview = servo.new_webview(url);
        webview.set_delegate(Rc::new(Web));

        glib::idle_add_local(move || {
            servo.spin_event_loop();
            servo.present();
            glib::ControlFlow::Continue
        });

        area.connect_render(move |_, _| {
            webview.composite();
            Propagation::Stop
        });
    });

    // Create a window and set the title
    let window = ApplicationWindow::builder()
        .application(app)
        .title("ServoGTK")
        .child(&gl_area)
        .build();

    // Present window
    window.present();
}

struct GTKRenderingContext {
    gl_area: GLArea,
    gl: Rc<dyn gl::Gl>,
}

impl RenderingContext for GTKRenderingContext {
    fn resize(&self, _size: servo::euclid::default::Size2D<i32>) {
        // GLArea should resize itself
    }

    fn present(&self) {
        self.gl_area.queue_render();
    }

    fn make_current(&self) -> Result<(), servo::webrender_traits::rendering_context::Error> {
        self.gl_area.make_current();
        Ok(())
    }

    fn framebuffer_object(&self) -> u32 {
        let mut fbo = [0];
        unsafe {
            self.gl.get_integer_v(gl::FRAMEBUFFER_BINDING, &mut fbo);
        }
        fbo[0] as u32
    }

    fn gl_api(&self) -> Rc<dyn gl::Gl> {
        self.gl.clone()
    }

    fn gl_version(&self) -> servo::webrender_traits::rendering_context::GLVersion {
        let (major, minor) = self
            .gl_area
            .context()
            .map(|c| c.version())
            .unwrap_or_default();
        if self.gl_area.api() == GLAPI::GL {
            GLVersion::GL(major as u8, minor as u8)
        } else {
            GLVersion::GLES(major as u8, minor as u8)
        }
    }
}

impl WindowMethods for GTKRenderingContext {
    fn get_coordinates(&self) -> servo::compositing::windowing::EmbedderCoordinates {
        let scale = Scale::new(self.gl_area.scale_factor() as f32);
        let (w, h) = self.gl_area.size_request();
        dbg!(w, h);
        let size = Size2D::new(w, h);
        let fsize = Size2D::new(w, h);
        EmbedderCoordinates {
            hidpi_factor: scale,
            screen_size: size,
            available_screen_size: size,
            window_rect: Box2D::from_size(size),
            framebuffer: fsize,
            viewport: Box2D::from_size(Size2D::new(w, h)),
        }
    }

    fn set_animation_state(&self, _state: servo::compositing::windowing::AnimationState) {}
}

struct EmbedderDelegate(Waker);

impl EmbedderMethods for EmbedderDelegate {
    fn create_event_loop_waker(&mut self) -> Box<dyn servo::EventLoopWaker> {
        self.0.clone_box()
    }
}

#[derive(Clone, Debug)]
struct Waker(());

impl EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }

    fn wake(&self) {
        // let _ = self.0.send(()).await;
    }
}

struct Web;

impl WebViewDelegate for Web {
    fn notify_ready_to_show(&self, webview: WebView) {
        webview.focus();
        webview.move_resize(Box2D::from_size(Size2D::new(800., 600.)));
        webview.raise_to_top(true);
    }
}

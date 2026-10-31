//! Spike S4: a meter-like window on GNOME Wayland, through XWayland.
//!
//!   meter-spike [--seconds N] [--bottom PIXELS] [--solid]
//!
//! It opens a 320x48 window at the bottom center of the X screen:
//! override-redirect (no title bar, the window manager does not place or
//! focus it), 32-bit ARGB (rounded, see-through corners), and an empty
//! input shape (clicks go to the window below). It animates level bars,
//! like the real meter, for `--seconds` (default 60).
//!
//! `--solid` keeps the input shape, so clicks hit the window: to compare.
//!
//! Questions (tasks/todo.md, S4): does it stay on top of native Wayland
//! windows (gedit, Firefox) when they are clicked? Does it take focus?
//! Do clicks pass through?

use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    ColormapAlloc, ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask, ImageFormat, VisualClass,
    WindowClass,
};
use x11rb::protocol::Event;

const WIDTH: u16 = 320;
const HEIGHT: u16 = 48;
const DEFAULT_SECONDS: u64 = 60;
const DEFAULT_BOTTOM: i16 = 96;
const FRAME: Duration = Duration::from_millis(50);
const BARS: usize = 24;

#[derive(Debug, PartialEq)]
struct Args {
    seconds: u64,
    bottom: i16,
    solid: bool,
}

fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(msg) => {
            eprintln!("{msg}");
            eprintln!("usage: meter-spike [--seconds N] [--bottom PIXELS] [--solid]");
            std::process::exit(2);
        }
    };
    if let Err(msg) = run(&args) {
        eprintln!("FAILED: {msg}");
        std::process::exit(1);
    }
}

fn run(args: &Args) -> Result<(), String> {
    let (conn, screen_num) = x11rb::connect(None).map_err(err("connect to X (XWayland)"))?;
    let screen = &conn.setup().roots[screen_num];
    println!("X screen {}x{}", screen.width_in_pixels, screen.height_in_pixels);

    // A 32-bit TrueColor visual gives per-pixel alpha.
    let visual = screen
        .allowed_depths
        .iter()
        .filter(|d| d.depth == 32)
        .flat_map(|d| d.visuals.iter())
        .find(|v| v.class == VisualClass::TRUE_COLOR)
        .ok_or("no 32-bit visual")?
        .visual_id;
    let colormap = conn.generate_id().map_err(err("id"))?;
    conn.create_colormap(ColormapAlloc::NONE, colormap, screen.root, visual).map_err(err("colormap"))?;

    let x = (screen.width_in_pixels as i16 - WIDTH as i16) / 2;
    let y = screen.height_in_pixels as i16 - HEIGHT as i16 - args.bottom;
    let window = conn.generate_id().map_err(err("id"))?;
    let aux = CreateWindowAux::new()
        .background_pixel(0)
        .border_pixel(0)
        .colormap(colormap)
        .override_redirect(1)
        .event_mask(EventMask::EXPOSURE | EventMask::BUTTON_PRESS | EventMask::FOCUS_CHANGE);
    conn.create_window(32, window, screen.root, x, y, WIDTH, HEIGHT, 0, WindowClass::INPUT_OUTPUT, visual, &aux)
        .map_err(err("create window"))?;

    if !args.solid {
        // An empty input shape: the window gets no clicks; they go below.
        conn.shape_rectangles(
            shape::SO::SET,
            shape::SK::INPUT,
            x11rb::protocol::xproto::ClipOrdering::UNSORTED,
            window,
            0,
            0,
            &[],
        )
        .map_err(err("input shape"))?;
    }
    conn.map_window(window).map_err(err("map"))?;
    let gc = conn.generate_id().map_err(err("id"))?;
    conn.create_gc(gc, window, &CreateGCAux::new()).map_err(err("gc"))?;
    conn.flush().map_err(err("flush"))?;
    println!(
        "Meter at {x},{y} ({}). Running {} s.",
        if args.solid { "clicks hit it" } else { "click-through" },
        args.seconds
    );

    let started = Instant::now();
    let mut frame = 0u32;
    while started.elapsed() < Duration::from_secs(args.seconds) {
        // Raise it again each frame, in case another window covered it.
        conn.configure_window(window, &x11rb::protocol::xproto::ConfigureWindowAux::new().stack_mode(x11rb::protocol::xproto::StackMode::ABOVE))
            .map_err(err("raise"))?;
        let pixels = draw(frame);
        conn.put_image(ImageFormat::Z_PIXMAP, window, gc, WIDTH, HEIGHT, 0, 0, 0, 32, &pixels)
            .map_err(err("put_image"))?;
        conn.flush().map_err(err("flush"))?;
        while let Some(event) = conn.poll_for_event().map_err(err("event"))? {
            match event {
                Event::ButtonPress(_) => println!("{:>5.1} s  the meter got a click", started.elapsed().as_secs_f32()),
                Event::FocusIn(_) => println!("{:>5.1} s  the meter got the focus", started.elapsed().as_secs_f32()),
                _ => {}
            }
        }
        frame += 1;
        std::thread::sleep(FRAME);
    }
    println!("Done.");
    Ok(())
}

/// One frame as premultiplied BGRA, the byte order of a 32-bit X image on
/// little-endian machines: a dark rounded pill, a red dot, moving bars.
fn draw(frame: u32) -> Vec<u8> {
    let (w, h) = (WIDTH as f32, HEIGHT as f32);
    let mut out = vec![0u8; WIDTH as usize * HEIGHT as usize * 4];
    for py in 0..HEIGHT as usize {
        for px in 0..WIDTH as usize {
            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
            let mut rgba = [0.0f32; 4];
            let pill = coverage(rounded_rect(x, y, w, h, h / 2.0));
            if pill > 0.0 {
                rgba = blend(rgba, [0.11, 0.11, 0.12], 0.9 * pill);
                let dot = coverage(((x - 24.0).powi(2) + (y - h / 2.0).powi(2)).sqrt() - 6.0);
                rgba = blend(rgba, [0.9, 0.2, 0.2], dot * pill);
                if let Some(level) = bar_level(x, frame) {
                    let bar_h = 4.0 + level * 24.0;
                    let inside = (y - h / 2.0).abs() < bar_h / 2.0;
                    if inside {
                        rgba = blend(rgba, [0.9, 0.9, 0.9], pill);
                    }
                }
            }
            let i = (py * WIDTH as usize + px) * 4;
            out[i] = (rgba[2] * 255.0) as u8;
            out[i + 1] = (rgba[1] * 255.0) as u8;
            out[i + 2] = (rgba[0] * 255.0) as u8;
            out[i + 3] = (rgba[3] * 255.0) as u8;
        }
    }
    out
}

/// The level of the bar at `x`, if `x` is inside a bar.
fn bar_level(x: f32, frame: u32) -> Option<f32> {
    let left = 48.0;
    let pitch = 10.0;
    let i = ((x - left) / pitch).floor();
    if i < 0.0 || i >= BARS as f32 || (x - left) - i * pitch > 5.0 {
        return None;
    }
    let t = frame as f32 * 0.25 + i * 0.6;
    Some(0.5 + 0.5 * t.sin() * (t * 0.37).cos())
}

/// Signed distance to a rounded rectangle that fills the window.
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    let qx = (x - w / 2.0).abs() - (w / 2.0 - r);
    let qy = (y - h / 2.0).abs() - (h / 2.0 - r);
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

/// Antialiased coverage of a shape from its signed distance.
fn coverage(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

/// Paint a colour with `alpha` over premultiplied `dst`.
fn blend(dst: [f32; 4], rgb: [f32; 3], alpha: f32) -> [f32; 4] {
    let keep = 1.0 - alpha;
    [
        rgb[0] * alpha + dst[0] * keep,
        rgb[1] * alpha + dst[1] * keep,
        rgb[2] * alpha + dst[2] * keep,
        alpha + dst[3] * keep,
    ]
}

fn parse_args(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = Args { seconds: DEFAULT_SECONDS, bottom: DEFAULT_BOTTOM, solid: false };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--seconds" => args.seconds = it.next().and_then(|v| v.parse().ok()).ok_or("--seconds needs a number")?,
            "--bottom" => args.bottom = it.next().and_then(|v| v.parse().ok()).ok_or("--bottom needs a number")?,
            "--solid" => args.solid = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn err<E: std::fmt::Display>(what: &'static str) -> impl Fn(E) -> String {
    move |e| format!("{what}: {e}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, String> {
        parse_args(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn arguments() {
        assert_eq!(parse(&[]).unwrap(), Args { seconds: DEFAULT_SECONDS, bottom: DEFAULT_BOTTOM, solid: false });
        assert_eq!(
            parse(&["--seconds", "5", "--bottom", "10", "--solid"]).unwrap(),
            Args { seconds: 5, bottom: 10, solid: true }
        );
        assert!(parse(&["--seconds"]).is_err());
        assert!(parse(&["--top"]).is_err());
    }

    #[test]
    fn frame_has_one_pixel_per_point() {
        assert_eq!(draw(0).len(), WIDTH as usize * HEIGHT as usize * 4);
    }

    #[test]
    fn corners_are_transparent_and_the_middle_is_opaque() {
        let pixels = draw(0);
        let alpha = |x: usize, y: usize| pixels[(y * WIDTH as usize + x) * 4 + 3];
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(WIDTH as usize - 1, HEIGHT as usize - 1), 0);
        assert!(alpha(WIDTH as usize / 2, HEIGHT as usize / 2) > 200);
    }

    #[test]
    fn premultiplied_colour_never_exceeds_alpha() {
        let pixels = draw(3);
        for px in pixels.chunks(4) {
            assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3]);
        }
    }
}

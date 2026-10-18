mod ui;
use glorious_core_rs::device;
fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let dry = args.iter().any(|a| a == "--dry-run")
        || std::env::var("GLORIOUS_CORE_DRY_RUN").is_ok_and(|v| v == "1" || v == "true");
    args.retain(|a| a != "--dry-run");
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "inglorious-core [--dry-run] [info | profile <1..3> | effect <0..18> [speed%] [brightness%] [RRGGBB]]\nWith no subcommand, opens the GUI. GLORIOUS_CORE_DRY_RUN=1 also prevents hardware access."
        );
        return Ok(());
    }
    if !args.is_empty() {
        return device::cli(dry, &args);
    }
    let state = device::load_state()?;
    eframe::run_native(
        "Inglorious Core Linux",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_app_id("inglorious-core")
                .with_inner_size([1360., 840.])
                .with_min_inner_size([680., 480.]),
            // With vsync the GL swap waits for a frame callback, which Hyprland never sends to a
            // window on a hidden workspace; the blocked UI thread then misses pings and gets an
            // "Application Not Responding" dialog (e.g. on a theme switch). Frames are on-demand anyway.
            vsync: false,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, state, dry)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

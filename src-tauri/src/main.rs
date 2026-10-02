#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK's DMA-BUF renderer can be killed during startup on hybrid
    // Intel/NVIDIA systems. Set this before Tauri initializes WebKit without
    // overriding a value supplied by the user's launch environment.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    omacal_lib::run()
}

use tauri::{
    AppHandle, Runtime,
    menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

pub const QUIT_MENU_ID: &str = "quit-with-note-confirmation";
const QUIT_ACCELERATOR: &str = "CmdOrCtrl+Q";
const APPLICATION_SUBMENU_INDEX: usize = 0;

pub fn menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;
    let name = app
        .config()
        .product_name
        .clone()
        .unwrap_or_else(|| app.package_info().name.clone());
    let application = Submenu::with_items(
        app,
        &name,
        true,
        &[
            &PredefinedMenuItem::about(
                app,
                None,
                Some(AboutMetadata {
                    name: Some(name.clone()),
                    version: Some(app.package_info().version.to_string()),
                    ..Default::default()
                }),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                QUIT_MENU_ID,
                format!("Quit {name}"),
                true,
                Some(QUIT_ACCELERATOR),
            )?,
        ],
    )?;
    menu.remove_at(APPLICATION_SUBMENU_INDEX)?
        .ok_or_else(|| std::io::Error::other("The default macOS application submenu is missing"))?;
    menu.prepend(&application)?;
    Ok(menu)
}

# Desktop widgets - install

1. Extract this zip INTO your Dynamic Island workspace folder (the one with Dynamic_Island, taskbar_app...).
   Say yes when asked to overwrite: it replaces `Cargo.toml`, `shared/` and `settings_app/`, and adds `widgets_app/`.
   (Your `Dynamic_Island/` and `taskbar_app/` folders are not touched. If your own versions of shared/settings_app have changes since, tell me before overwriting.)
2. Open a terminal in that folder and run:  `cargo build --release`
3. Run `target\release\settings-app.exe`, go to the **Widgets** page.
   Click a card to add a widget. It turns the widgets program on for you (and starts it with Windows).
4. Each widget has its own card under "Your widgets": size, look, time zone, city, files, etc.
   Closing a widget with its X on the desktop removes it from the list too.

Notes
- Widgets sit behind your apps. Hover a widget to get move / close buttons and the resize corner at the bottom right.
- Video uses Windows Media Foundation (MP4 works best). If the build fails inside `video.rs`, open
  `widgets_app/Cargo.toml` and change `default = ["video"]` to `default = []`, then build again (images and GIFs still work).
- If a video looks upside down, turn on "Flip video upside down" in that widget's settings.
- The files `widgets.json` and `config.json` live in `%APPDATA%\DynamicIsland`.

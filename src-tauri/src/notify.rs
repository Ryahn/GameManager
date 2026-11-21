//! Native notifications.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

pub fn send(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        log::warn!("notification failed: {e}");
    }
}

/// "Summer Nights, Crimson Haven and 3 more"
pub fn summarize(titles: &[String]) -> String {
    match titles {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} and {b}"),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::summarize;

    #[test]
    fn summarizes_titles() {
        let t = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(summarize(&t(&["A"])), "A");
        assert_eq!(summarize(&t(&["A", "B"])), "A and B");
        assert_eq!(summarize(&t(&["A", "B", "C", "D"])), "A, B and 2 more");
    }
}

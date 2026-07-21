#[cfg(test)]
use crate::constellations::Constellations;

#[test]
fn test_view_timeline_renders_without_panicking() {
    let constellations = Constellations::mock();
    let _element = constellations.view_timeline();
}

#[test]
fn test_view_threaded_timeline_renders_without_panicking() {
    let constellations = Constellations::mock();
    let _element = constellations.view_threaded_timeline();
}

#[test]
fn test_view_main_content_renders_without_panicking() {
    let constellations = Constellations::mock();
    let _element = constellations.view_main_content();
}

#[test]
fn test_view_composer_renders_without_panicking() {
    let constellations = Constellations::mock();
    let _element = constellations.view_composer();
}

#[test]
fn test_view_search_results_renders_without_panicking() {
    let constellations = Constellations::mock();
    let _element = constellations.view_search_results();
}

#[test]
fn test_view_members_panel_renders_without_panicking() {
    let constellations = Constellations::mock();
    let _element = constellations.view_members_panel();
}

#[test]
fn test_view_pinned_panel_renders_without_panicking() {
    let constellations = Constellations::mock();
    let _element = constellations.view_pinned_panel();
}

#[cfg(test)]
use crate::view::error::view_error;

#[test]
fn test_view_error_renders_without_panicking_with_str() {
    // Smoke test for static str
    let _element = view_error("Test Error");
}

#[test]
fn test_view_error_renders_without_panicking_with_string() {
    // Smoke test for owned String
    let _element = view_error(String::from("Another Test Error"));
}

#[test]
fn test_view_error_renders_without_panicking_with_empty_string() {
    // Smoke test for empty string
    let _element = view_error("");
}

#[test]
fn test_view_error_renders_without_panicking_with_long_string() {
    // Smoke test for long string
    let long_string = "a".repeat(1000);
    let _element = view_error(long_string);
}

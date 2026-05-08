use taffy::prelude::TaffyAuto;
use taffy::style_helpers::{minmax, fr, auto, length};
use taffy::style::{TrackSizingFunction, GridTemplateComponent};

/// Parse a grid template string like "1fr 1fr 1fr" or "100px 200px auto"
/// Supports: Npx (pixels), Nfr (fractions), auto
pub fn parse_grid_template(template: &str) -> Vec<GridTemplateComponent<String>> {
    let mut tracks = Vec::new();

    for part in template.split_whitespace() {
        let track: TrackSizingFunction = if part == "auto" {
            minmax(auto(), auto())
        } else if part.ends_with("fr") {
            let value: f32 = part.trim_end_matches("fr").parse().unwrap_or(1.0);
            minmax(length(0.0), fr(value))
        } else if part.ends_with("px") {
            let value: f32 = part.trim_end_matches("px").parse().unwrap_or(0.0);
            minmax(length(value), length(value))
        } else {
            // Try parsing as a plain number (treat as pixels)
            let value: f32 = part.parse().unwrap_or(0.0);
            minmax(length(value), length(value))
        };
        tracks.push(GridTemplateComponent::from(track));
    }

    tracks
}

/// Parse a CSS-like dimension string into a Taffy Dimension
/// Supports: N% (percent), auto
pub fn parse_dimension(value: &str) -> taffy::Dimension {
    use taffy::Dimension;
    let trimmed = value.trim();
    if trimmed == "auto" {
        Dimension::AUTO
    } else if trimmed.ends_with('%') {
        let num: f32 = trimmed.trim_end_matches('%').parse().unwrap_or(0.0);
        Dimension::percent(num / 100.0)
    } else {
        Dimension::AUTO
    }
}

/// Parse a CSS-like dimension string into LengthPercentageAuto
/// Used for margin and inset properties (supports auto)
pub fn parse_length_percentage_auto(value: &str) -> taffy::LengthPercentageAuto {
    use taffy::LengthPercentageAuto;
    let trimmed = value.trim();
    if trimmed == "auto" {
        LengthPercentageAuto::AUTO
    } else if trimmed.ends_with('%') {
        let num: f32 = trimmed.trim_end_matches('%').parse().unwrap_or(0.0);
        LengthPercentageAuto::percent(num / 100.0)
    } else {
        LengthPercentageAuto::AUTO
    }
}

/// Parse a CSS-like dimension string into LengthPercentage
/// Used for padding properties (no auto support)
pub fn parse_length_percentage(value: &str) -> taffy::LengthPercentage {
    use taffy::LengthPercentage;
    let trimmed = value.trim();
    if trimmed.ends_with('%') {
        let num: f32 = trimmed.trim_end_matches('%').parse().unwrap_or(0.0);
        LengthPercentage::percent(num / 100.0)
    } else {
        LengthPercentage::length(0.0)
    }
}
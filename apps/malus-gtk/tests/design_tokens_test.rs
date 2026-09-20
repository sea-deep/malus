//! Mathematical consistency audit for malus-gtk design tokens.

use malus_gtk::design::*;
use malus_gtk::model::*;

#[test]
fn test_base_grid_alignment() {
    let spacings = [
        SPACING_XXS,
        SPACING_XS,
        SPACING_SM,
        SPACING_MD,
        SPACING_LG,
        SPACING_XL,
        SPACING_XXL,
        SPACING_XXXL,
    ];

    for s in spacings {
        assert_eq!(s % 4, 0, "Spacing token {s} must be a multiple of 4px");
    }

    // 8px primary rhythm check on major layout dimensions
    assert_eq!(
        SIDEBAR_WIDTH_NORMAL as i32 % 8,
        0,
        "Sidebar width must be an 8px multiple"
    );
    assert_eq!(
        PAGE_PADDING_NORMAL % 8,
        0,
        "Page padding must be an 8px multiple"
    );
    assert_eq!(
        BREAKPOINT_COLLAPSE_NAVIGATION as i32 % 4,
        0,
        "Navigation collapse breakpoint must be a 4px multiple"
    );
}

#[test]
fn test_control_and_icon_hierarchy() {
    let controls = [CONTROL_SM, CONTROL_MD, CONTROL_LG];
    for c in controls {
        assert_eq!(c % 4, 0, "Control size {c} must be a multiple of 4");
    }

    let icons = [ICON_GLYPH_SM, ICON_GLYPH_MD, ICON_GLYPH_LG];
    for i in icons {
        assert_eq!(i % 4, 0, "Icon size {i} must be a multiple of 4");
    }

    // Hit targets must be at least 40px for normal controls
    const { assert!(CONTROL_MD >= 40) };
    const { assert!(CONTROL_LG >= 48) };
}

#[test]
fn test_radii_scale() {
    let radii = [RADIUS_SM, RADIUS_MD, RADIUS_LG, RADIUS_XL];
    assert_eq!(radii, [6, 8, 10, 12]);
}

#[test]
fn test_time_formatting_precision() {
    assert_eq!(format_time(0), "0:00");
    assert_eq!(format_time(60_000), "1:00");
    assert_eq!(format_time(125_000), "2:05");
    assert_eq!(format_remaining_time(30_000, 125_000), "-1:35");
}

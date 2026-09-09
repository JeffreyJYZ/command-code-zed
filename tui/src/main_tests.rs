use crate::redraw_frame;

#[test]
fn redraw_grow_rewrites_from_old_top() {
    let mut buf: Vec<u8> = Vec::new();
    // prev = 5 (error frame), new = 14 (full frame)
    let frame = (0..14).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
    let n = redraw_frame(&mut buf, &frame, 5);
    assert_eq!(n, 14);
    let s = String::from_utf8(buf).unwrap();
    assert!(s.starts_with("\x1b[4F"), "must move up prev-1 to old top: {s:?}");
    assert_eq!(s.matches("line13").count(), 1);
    assert!(!s.contains("\x1b[1B"), "no shrink needed: {s:?}");
}

#[test]
fn redraw_shrink_clears_stale_rows_and_returns_to_new_bottom() {
    let mut buf: Vec<u8> = Vec::new();
    // prev = 14 (full frame), new = 5 (error frame): the bug case that
    // stair-stepped the screen. Old bug: each \x1b[2K\r\n pad cleared the
    // freshly-written status row and \x1b[1F landed prev-n-1 rows short.
    let frame = (0..5).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
    let n = redraw_frame(&mut buf, &frame, 14);
    assert_eq!(n, 5);
    let s = String::from_utf8(buf).unwrap();
    // move up 13 to old top, draw 5 lines
    assert!(s.starts_with("\x1b[13F"), "must move up prev-1: {s:?}");
    assert_eq!(s.matches("\n").count(), 4, "exactly n-1 line feeds: {s:?}");
    // shrink: down one (status survives), clear 8 stale rows, clear old
    // bottom, then up (prev-n) = 9 back to the new bottom.
    let shrink = s.split("line4").nth(1).unwrap();
    assert!(shrink.starts_with("\x1b[1B"), "down past frame, no clear: {shrink:?}");
    assert_eq!(shrink.matches("\x1b[2K\x1b[1B").count(), 8, "stale rows: {shrink:?}");
    assert!(shrink.ends_with("\x1b[2K\x1b[9F"), "clear old bottom + up prev-n: {shrink:?}");
}

#[test]
fn redraw_no_previous_frame_just_draws() {
    let mut buf: Vec<u8> = Vec::new();
    let n = redraw_frame(&mut buf, "a\nb", 0);
    assert_eq!(n, 2);
    let s = String::from_utf8(buf).unwrap();
    assert_eq!(s, "\x1b[2K\ra\n\x1b[2K\rb", "no up-move on first frame, no trailing nl: {s:?}");
}

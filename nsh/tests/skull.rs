#[test]
fn couts_taps() {
    let config = nsh_engine::EngineConfig::new(nsh_engine::Root::of::<nsh::App>());
    let mut app = nsh_testing::Headless::start(config).unwrap();
    app.wait_for("Count: 0").unwrap();
    app.press("Tap").unwrap();
    app.press("Tap").unwrap();
    assert!(app.screen().texts().contains(&"Count: 2"));
}

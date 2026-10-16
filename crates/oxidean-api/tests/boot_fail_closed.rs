//! D-14 fail-closed boot checkpoint (AUTH-06).
//! Process exit on ENV admin seed Err — no wizard fallback.

#[test]
fn d14_fail_closed_exits_on_admin_seed_err() {
    let src = include_str!("../src/main.rs");
    let call = "seed::maybe_seed_admin";
    assert!(
        src.contains(call),
        "boot must call maybe_seed_admin when DB is configured"
    );

    // Use the call site (not the earlier D-04 comment that also names maybe_seed_admin).
    let after_call = src
        .split(call)
        .nth(1)
        .expect("seed::maybe_seed_admin call site");
    let seed_err_block = after_call
        .split("let bind")
        .next()
        .expect("bind follows seed block");

    assert!(
        seed_err_block.contains("std::process::exit(1)"),
        "D-14: admin seed Err must exit(1) (fail closed)"
    );
    assert!(
        !seed_err_block.contains("serve_wizard_fallback"),
        "D-14: must not serve wizard fallback when seed Err"
    );
    assert!(
        seed_err_block.contains("admin seed failed"),
        "D-14: seed failure must be reported before exit"
    );
}

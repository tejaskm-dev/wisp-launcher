//! Checks playit.gg's claim service answers as Wisp expects, with a fresh code nobody approves
//! (so nothing is linked). Tester-build feature only:
//!
//!   cargo run -p wisp-core --features playit --example playit_check

#[tokio::main]
async fn main() {
    let code = wisp_core::playit::claim_code();
    println!("Claim page (not opened): {}", wisp_core::playit::claim_url(&code));
    let waiting = tokio::time::timeout(std::time::Duration::from_secs(6), wisp_core::playit::claim(&code, "Wisp check", &|step| println!("  step: {step:?}"))).await;
    match waiting {
        Err(_) => println!("Still waiting for approval after 6 s, as expected."),
        Ok(result) => println!("Ended: {:?}", result.map(|_| "a secret")),
    }
}

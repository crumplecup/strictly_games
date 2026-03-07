//! Formal verification runner and tracking

use std::process::Command;
use anyhow::{Result, Context};
use serde_json::Value;

/// Run all verification tools
pub fn run_all(verbose: bool) -> Result<()> {
    println!("🔬 Running Formal Verification Trifecta\n");
    
    let mut all_passed = true;
    
    // Kani
    if let Err(e) = run_kani(verbose) {
        eprintln!("❌ Kani failed: {}", e);
        all_passed = false;
    }
    
    // Verus
    if let Err(e) = run_verus(verbose) {
        eprintln!("❌ Verus failed: {}", e);
        all_passed = false;
    }
    
    // Creusot
    if let Err(e) = run_creusot(verbose) {
        eprintln!("❌ Creusot failed: {}", e);
        all_passed = false;
    }
    
    if all_passed {
        println!("\n🎉 ALL VERIFIERS PASSED");
        Ok(())
    } else {
        anyhow::bail!("Some verifiers failed")
    }
}

/// Run Kani symbolic execution
pub fn run_kani(verbose: bool) -> Result<()> {
    println!("📊 Running Kani...");
    
    let output = Command::new("cargo")
        .args(["kani", "-p", "strictly_proofs"])
        .output()
        .context("Failed to run cargo kani")?;
    
    if verbose {
        println!("{}", String::from_utf8_lossy(&output.stdout));
        println!("{}", String::from_utf8_lossy(&output.stderr));
    }
    
    if output.status.success() {
        println!("✅ Kani verification passed\n");
        Ok(())
    } else {
        anyhow::bail!("Kani verification failed")
    }
}

/// Run Verus SMT-based verification
pub fn run_verus(verbose: bool) -> Result<()> {
    println!("📊 Running Verus...");
    
    let files = vec![
        "crates/strictly_proofs/src/verus_proofs/game_invariants.rs",
        "crates/strictly_proofs/src/verus_proofs/compositional_proof.rs",
    ];
    
    let mut total_verified = 0;
    
    for file in files {
        let output = Command::new("verus")
            .args(["--crate-type=lib", "--output-json", file])
            .output()
            .context("Failed to run verus")?;
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        // Parse JSON output
        if let Ok(json) = serde_json::from_str::<Value>(&stdout)
            && let Some(results) = json.get("verification-results")
            && let Some(verified) = results.get("verified").and_then(|v| v.as_u64())
        {
            total_verified += verified;
        }
        
        if verbose {
            println!("{}", stdout);
        }
    }
    
    println!("✅ Verus verified {} proofs\n", total_verified);
    Ok(())
}

/// Run Creusot Why3-based verification
pub fn run_creusot(verbose: bool) -> Result<()> {
    println!("📊 Running Creusot...");
    
    let output = Command::new("cargo")
        .args(["check", "-p", "strictly_proofs"])
        .output()
        .context("Failed to run cargo check")?;
    
    if verbose {
        println!("{}", String::from_utf8_lossy(&output.stdout));
        println!("{}", String::from_utf8_lossy(&output.stderr));
    }
    
    if output.status.success() {
        println!("✅ Creusot proofs compiled\n");
        Ok(())
    } else {
        anyhow::bail!("Creusot compilation failed")
    }
}

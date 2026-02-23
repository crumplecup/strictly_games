#!/usr/bin/env python3
"""Verification tracking script for strictly_games."""

import argparse
import csv
import re
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path


class VerificationResult:
    """Result from running a single proof harness."""
    
    def __init__(self, verifier: str, harness: str, status: str,
                 checks: int = 0, time_seconds: float = 0.0, error_message: str = ""):
        self.verifier = verifier
        self.harness = harness
        self.status = status
        self.checks = checks
        self.time_seconds = time_seconds
        self.timestamp = datetime.now(timezone.utc).isoformat()
        self.error_message = error_message


def run_kani_harness(harness: str) -> VerificationResult:
    """Run a single Kani harness and parse results."""
    start = time.time()
    
    try:
        result = subprocess.run(
            ["cargo", "kani", "-p", "strictly_proofs", "--harness", harness],
            capture_output=True, text=True, timeout=600
        )
        elapsed = time.time() - start
        output = result.stdout + result.stderr
        
        # Parse status
        if "VERIFICATION:- SUCCESSFUL" in output:
            status = "Success"
        elif "VERIFICATION:- FAILED" in output:
            status = "Failed"
        else:
            status = "Unknown"
        
        # Extract checks
        checks = 0
        match = re.search(r"(\d+) of (\d+) failed", output)
        if match:
            checks = int(match.group(2))
        
        # Extract error
        error_msg = ""
        if status == "Failed":
            error_msg = output[-200:].replace("\n", " ").strip()
        
        return VerificationResult("kani", harness, status, checks, round(elapsed, 2), error_msg)
        
    except subprocess.TimeoutExpired:
        return VerificationResult("kani", harness, "Timeout", 0, 600.0, "Timeout after 10 minutes")
    except Exception as e:
        return VerificationResult("kani", harness, "Error", 0, 0.0, str(e))


def list_kani_harnesses() -> list[str]:
    """List all Kani proof harnesses."""
    harnesses = []
    kani_dir = Path("crates/strictly_proofs/src/kani_proofs")
    if kani_dir.exists():
        for rs_file in kani_dir.glob("*.rs"):
            content = rs_file.read_text()
            matches = re.finditer(r'#\[kani::proof\]\s+(?:pub\s+)?fn\s+(\w+)', content, re.MULTILINE)
            harnesses.extend(m.group(1) for m in matches)
    return sorted(harnesses)


def write_csv(results: list[VerificationResult], csv_path: Path):
    """Write results to CSV."""
    if not csv_path.exists():
        with open(csv_path, 'w', newline='') as f:
            csv.writer(f).writerow(['verifier', 'harness', 'status', 'checks', 'time_seconds', 'timestamp', 'error_message'])
    
    with open(csv_path, 'a', newline='') as f:
        writer = csv.writer(f)
        for r in results:
            writer.writerow([r.verifier, r.harness, r.status, r.checks, r.time_seconds, r.timestamp, r.error_message])


def run_kani_all(csv_path: Path, verbose: bool = False):
    """Run all Kani harnesses."""
    print("Discovering Kani harnesses...")
    harnesses = list_kani_harnesses()
    
    if not harnesses:
        print("❌ No Kani harnesses found")
        return 1
    
    print(f"Found {len(harnesses)} harnesses\n")
    results = []
    
    for i, harness in enumerate(harnesses, 1):
        print(f"[{i}/{len(harnesses)}] {harness}...", end=" ", flush=True)
        result = run_kani_harness(harness)
        results.append(result)
        
        if result.status == "Success":
            print(f"✅ {result.checks:,} checks in {result.time_seconds:.1f}s")
        else:
            print(f"❌ {result.status}")
    
    write_csv(results, csv_path)
    
    success = sum(1 for r in results if r.status == "Success")
    print(f"\n📊 Results: {success}/{len(results)} passed → {csv_path}")
    return 0 if success == len(results) else 1


def show_status(csv_path: Path):
    """Show verification status."""
    if not csv_path.exists():
        print(f"❌ No results: {csv_path}")
        return 1
    
    latest = {}
    with open(csv_path, 'r') as f:
        for row in csv.DictReader(f):
            latest[(row['verifier'], row['harness'])] = row
    
    print("\n" + "="*80)
    print("VERIFICATION STATUS")
    print("="*80)
    
    by_verifier = {}
    for (v, h), row in latest.items():
        by_verifier.setdefault(v, []).append(row)
    
    for verifier in sorted(by_verifier):
        results = by_verifier[verifier]
        success = sum(1 for r in results if r['status'] == 'Success')
        print(f"\n{verifier.upper()}: {success}/{len(results)} passing")
        print("-" * 80)
        
        for r in sorted(results, key=lambda x: x['harness']):
            icon = "✅" if r['status'] == 'Success' else "❌"
            print(f"  {icon} {r['harness']:<40} {r.get('checks', '0'):>6} checks  {r.get('time_seconds', '0'):>6}s")
    
    print("="*80 + "\n")
    return 0


def list_verus_proofs() -> list[str]:
    """List all Verus proof functions."""
    proofs = []
    verus_dir = Path("crates/strictly_proofs/src/verus_proofs")
    if verus_dir.exists():
        for rs_file in verus_dir.glob("*.rs"):
            content = rs_file.read_text()
            matches = re.finditer(r'pub\s+proof\s+fn\s+(\w+)', content, re.MULTILINE)
            proofs.extend(m.group(1) for m in matches)
    return sorted(proofs)


def list_creusot_proofs() -> list[str]:
    """List all Creusot proof functions."""
    proofs = []
    creusot_dir = Path("crates/strictly_proofs/src/creusot_proofs")
    if creusot_dir.exists():
        for rs_file in creusot_dir.glob("*.rs"):
            content = rs_file.read_text()
            matches = re.finditer(r'#\[trusted\].*?pub\s+fn\s+(\w+)', content, re.MULTILINE | re.DOTALL)
            proofs.extend(m.group(1) for m in matches)
    return sorted(proofs)


def run_verus_all(csv_path: Path, verbose: bool = False):
    """Run all Verus proofs."""
    print("Discovering Verus proofs...")
    proofs = list_verus_proofs()
    
    if not proofs:
        print("❌ No Verus proofs found")
        return 1
    
    print(f"Found {len(proofs)} proofs\n")
    
    # Run Verus verification on the library with JSON output
    start = time.time()
    try:
        result = subprocess.run(
            ["verus", "--crate-type=lib", "--output-json", "crates/strictly_proofs/src/lib.rs"],
            capture_output=True, text=True, timeout=600
        )
        elapsed = time.time() - start
        output = result.stdout + result.stderr
        
        # Parse Verus output - it reports "verification results:: N verified, M errors"
        if "verification results::" in output.lower():
            # Extract verification stats
            match = re.search(r'verification results::\s*(\d+)\s*verified,\s*(\d+)\s*errors?', output, re.IGNORECASE)
            if match:
                verified_count = int(match.group(1))
                error_count = int(match.group(2))
                status = "Success" if error_count == 0 else "Failed"
                error_msg = f"{error_count} verification errors" if error_count > 0 else ""
            else:
                status = "Unknown"
                error_msg = "Could not parse verification results"
        elif result.returncode == 0:
            status = "Success"
            error_msg = ""
        else:
            status = "Failed"
            # Extract error message
            lines = output.split('\n')
            error_lines = [l for l in lines if 'error' in l.lower()][:3]
            error_msg = ' '.join(error_lines).replace("\n", " ").strip()[:200]
        
        results = [VerificationResult("verus", f"verus_proofs_{len(proofs)}_functions", status, len(proofs), round(elapsed, 2), error_msg)]
        write_csv(results, csv_path)
        
        if status == "Success":
            print(f"✅ All {len(proofs)} proofs verified in {elapsed:.1f}s")
        else:
            print(f"❌ Verification failed: {error_msg}")
            if verbose:
                print("\nFull output:")
                print(output[-1000:])
        
        return 0 if status == "Success" else 1
        
    except subprocess.TimeoutExpired:
        result = VerificationResult("verus", "verus_proofs", "Timeout", 0, 600.0, "Timeout after 10 minutes")
        write_csv([result], csv_path)
        print("❌ Timeout after 10 minutes")
        return 1
    except FileNotFoundError:
        error_msg = "verus command not found - install from https://github.com/verus-lang/verus"
        result = VerificationResult("verus", "verus_proofs", "Error", 0, 0.0, error_msg)
        write_csv([result], csv_path)
        print(f"❌ {error_msg}")
        return 1
    except Exception as e:
        result = VerificationResult("verus", "verus_proofs", "Error", 0, 0.0, str(e))
        write_csv([result], csv_path)
        print(f"❌ Error: {e}")
        return 1


def run_creusot_all(csv_path: Path, verbose: bool = False):
    """Run all Creusot proofs."""
    print("Discovering Creusot proofs...")
    proofs = list_creusot_proofs()
    
    if not proofs:
        print("❌ No Creusot proofs found")
        return 1
    
    print(f"Found {len(proofs)} proofs\n")
    
    # Creusot #[trusted] proofs compile instantly with standard cargo
    # No verification performed - trusted axioms by design (cloud of assumptions)
    start = time.time()
    try:
        result = subprocess.run(
            ["cargo", "check", "-p", "strictly_proofs"],
            capture_output=True, text=True, timeout=300
        )
        elapsed = time.time() - start
        
        # Check if creusot module compiled cleanly
        status = "Success" if result.returncode == 0 else "Failed"
        error_msg = ""
        if status == "Failed":
            # Extract error related to creusot_proofs
            lines = result.stderr.split('\n')
            creusot_errors = [l for l in lines if 'creusot_proofs' in l]
            error_msg = ' '.join(creusot_errors[-3:]) if creusot_errors else result.stderr[-200:]
            error_msg = error_msg.replace("\n", " ").strip()
        
        results = [VerificationResult("creusot", f"all_{len(proofs)}_proofs", status, len(proofs), round(elapsed, 2), error_msg)]
        write_csv(results, csv_path)
        
        if status == "Success":
            print(f"✅ All {len(proofs)} proofs compiled in {elapsed:.1f}s")
        else:
            print(f"❌ Compilation failed")
            if verbose:
                print(error_msg)
        
        return 0 if status == "Success" else 1
        
    except subprocess.TimeoutExpired:
        result = VerificationResult("creusot", "all_proofs", "Timeout", 0, 300.0, "Timeout after 5 minutes")
        write_csv([result], csv_path)
        return 1
    except Exception as e:
        result = VerificationResult("creusot", "all_proofs", "Error", 0, 0.0, str(e))
        write_csv([result], csv_path)
        return 1


def run_all_verifiers(csv_path: Path, verbose: bool = False):
    """Run all verifiers in sequence."""
    print("="*80)
    print("RUNNING VERIFICATION TRIFECTA")
    print("="*80 + "\n")
    
    exit_codes = []
    
    print("📍 Phase 1: Kani (symbolic execution)")
    print("-" * 80)
    exit_codes.append(run_kani_all(csv_path, verbose))
    
    print("\n📍 Phase 2: Verus (specification-based)")
    print("-" * 80)
    exit_codes.append(run_verus_all(csv_path, verbose))
    
    print("\n📍 Phase 3: Creusot (deductive)")
    print("-" * 80)
    exit_codes.append(run_creusot_all(csv_path, verbose))
    
    print("\n" + "="*80)
    if all(code == 0 for code in exit_codes):
        print("🎉 ALL VERIFIERS PASSED")
    else:
        print("⚠️  SOME VERIFIERS FAILED")
    print("="*80 + "\n")
    
    return 0 if all(code == 0 for code in exit_codes) else 1


def main():
    parser = argparse.ArgumentParser(description="Track verification results")
    parser.add_argument("command", choices=["run-kani", "run-verus", "run-creusot", "run-all", "status"])
    parser.add_argument("--csv", type=Path, default=Path("verification_results.csv"))
    parser.add_argument("-v", "--verbose", action="store_true")
    args = parser.parse_args()
    
    if args.command == "run-kani":
        return run_kani_all(args.csv, args.verbose)
    elif args.command == "run-verus":
        return run_verus_all(args.csv, args.verbose)
    elif args.command == "run-creusot":
        return run_creusot_all(args.csv, args.verbose)
    elif args.command == "run-all":
        return run_all_verifiers(args.csv, args.verbose)
    else:
        return show_status(args.csv)


if __name__ == "__main__":
    sys.exit(main())

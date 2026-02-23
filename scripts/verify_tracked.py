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
            ["cargo", "kani", "--harness", harness],
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
    kani_dir = Path("src/kani_proofs")
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


def main():
    parser = argparse.ArgumentParser(description="Track verification results")
    parser.add_argument("command", choices=["run-kani", "status"])
    parser.add_argument("--csv", type=Path, default=Path("verification_results.csv"))
    parser.add_argument("-v", "--verbose", action="store_true")
    args = parser.parse_args()
    
    if args.command == "run-kani":
        return run_kani_all(args.csv, args.verbose)
    else:
        return show_status(args.csv)


if __name__ == "__main__":
    sys.exit(main())

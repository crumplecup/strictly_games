#!/usr/bin/env python3
"""Generate verification dashboard from CSV results."""

import csv
import sys
from datetime import datetime
from pathlib import Path


def load_results(csv_path: Path) -> dict:
    """Load latest results by verifier."""
    latest = {}
    with open(csv_path, 'r') as f:
        for row in csv.DictReader(f):
            key = (row['verifier'], row['harness'])
            latest[key] = row
    return latest


def generate_markdown(results: dict) -> str:
    """Generate markdown dashboard."""
    by_verifier = {}
    for (v, h), row in results.items():
        by_verifier.setdefault(v, []).append(row)
    
    md = []
    md.append("# Formal Verification Dashboard")
    md.append("")
    md.append(f"**Last Updated**: {datetime.now().isoformat()}")
    md.append("")
    
    # Summary stats
    total_proofs = sum(len(rows) for rows in by_verifier.values())
    total_passing = sum(
        1 for rows in by_verifier.values()
        for r in rows if r['status'] == 'Success'
    )
    
    md.append("## Summary")
    md.append("")
    md.append(f"- **Total Verification Runs**: {total_proofs}")
    md.append(f"- **Passing**: {total_passing}/{total_proofs}")
    md.append(f"- **Coverage**: Kani + Verus + Creusot")
    md.append("")
    
    # Per-verifier breakdown
    md.append("## Verification Trifecta Status")
    md.append("")
    md.append("| Verifier | Approach | Proofs | Status | Time |")
    md.append("|----------|----------|--------|--------|------|")
    
    for verifier in sorted(by_verifier.keys()):
        rows = by_verifier[verifier]
        passing = sum(1 for r in rows if r['status'] == 'Success')
        total_time = sum(float(r.get('time_seconds', 0)) for r in rows)
        
        approach_map = {
            'kani': 'Symbolic Execution',
            'verus': 'SMT Specifications',
            'creusot': 'Deductive Trusted'
        }
        approach = approach_map.get(verifier, verifier)
        
        status_icon = "✅" if passing == len(rows) else "⚠️"
        
        # Extract proof count from harness name (e.g., "all_7_proofs" → 7)
        proof_count = rows[0].get('checks', len(rows))
        
        md.append(
            f"| {verifier.capitalize()} | {approach} | "
            f"{proof_count} | {status_icon} {passing}/{len(rows)} | "
            f"{total_time:.1f}s |"
        )
    
    md.append("")
    
    # Detailed results
    md.append("## Detailed Results")
    md.append("")
    
    for verifier in sorted(by_verifier.keys()):
        md.append(f"### {verifier.capitalize()}")
        md.append("")
        md.append("| Harness | Status | Checks | Time | Timestamp |")
        md.append("|---------|--------|--------|------|-----------|")
        
        for row in sorted(by_verifier[verifier], key=lambda r: r['harness']):
            icon = "✅" if row['status'] == 'Success' else "❌"
            timestamp = row['timestamp'][:10]  # Just date
            checks = row.get('checks', 'N/A')
            time_s = float(row.get('time_seconds', 0))
            
            md.append(
                f"| {row['harness']} | {icon} {row['status']} | "
                f"{checks} | {time_s:.1f}s | {timestamp} |"
            )
        
        md.append("")
    
    return "\n".join(md)


def main():
    csv_path = Path("verification_results.csv")
    
    if not csv_path.exists():
        print(f"❌ CSV not found: {csv_path}", file=sys.stderr)
        return 1
    
    results = load_results(csv_path)
    markdown = generate_markdown(results)
    
    output_path = Path("VERIFICATION_DASHBOARD.md")
    output_path.write_text(markdown)
    
    print(f"✅ Dashboard generated: {output_path}")
    print(markdown)
    return 0


if __name__ == "__main__":
    sys.exit(main())

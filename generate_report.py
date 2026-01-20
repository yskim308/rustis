import csv
import sys
from collections import defaultdict


def load_data(filename="benchmarks.csv"):
    with open(filename, "r") as f:
        reader = csv.DictReader(f)
        return list(reader)


def get_unique_notes(data):
    """Extracts unique run labels (notes) from the CSV."""
    return list(dict.fromkeys(row["Note"] for row in data))


def select_note(notes, prompt_text):
    """Renders a TUI menu for selecting a run label."""
    print("\033c", end="")  # Clear screen
    print(f"\033[95m=== {prompt_text} ===\033[0m\n")

    for idx, note in enumerate(notes):
        print(f"[\033[92m{idx + 1}\033[0m] \033[1m{note}\033[0m")

    if "Baseline" not in prompt_text:
        print(f"[\033[93ma\033[0m] \033[1mCompare ALL against Baseline\033[0m")

    print("\n[q] Quit")

    while True:
        choice = input(f"\nSelect > ").strip().lower()
        if choice == "q":
            sys.exit(0)

        if choice == "a" and "Baseline" not in prompt_text:
            return "ALL"

        try:
            idx = int(choice) - 1
            if 0 <= idx < len(notes):
                return notes[idx]
        except ValueError:
            pass

        print("\033[91mInvalid selection, try again.\033[0m")


def format_change(current, baseline, is_latency=False):
    """Calculates % change and returns a color-coded string."""
    if baseline == 0:
        return "N/A"

    diff = ((current - baseline) / baseline) * 100

    is_good = False
    if is_latency:
        is_good = diff <= 0
    else:
        is_good = diff >= 0

    icon = "🟢" if is_good else "🔴"
    sign = "+" if diff > 0 else ""
    return f"{icon} {sign}{diff:.2f}%"


def generate_table(data, baseline_note, target_note):
    # 1. Extract the Baseline Data
    baseline = {}
    for row in data:
        if row["Note"] == baseline_note:
            key = (row["Test Name"], row["Command"])
            baseline[key] = {
                "rps": float(row["RPS"]),
                "latency": float(row["Latency (p50)"]),
            }

    # 2. Extract Target Data
    target_rows = [row for row in data if row["Note"] == target_note]

    if not target_rows:
        return

    print(f"\n### 📊 Report: {target_note} vs {baseline_note}\n")
    # CLEANER HEADER: Removed Base columns
    print(f"| Test Name | Cmd | RPS | Δ RPS | Latency (ms) | Δ Lat |")
    print(f"| :--- | :--- | :--- | :--- | :--- | :--- |")

    for row in target_rows:
        key = (row["Test Name"], row["Command"])

        if key in baseline:
            base = baseline[key]
            curr_rps = float(row["RPS"])
            curr_lat = float(row["Latency (p50)"])

            rps_change = format_change(curr_rps, base["rps"], is_latency=False)
            lat_change = format_change(curr_lat, base["latency"], is_latency=True)

            r_new = f"{curr_rps:,.0f}"
            l_new = f"{curr_lat:.3f}"

            print(
                f"| {row['Test Name']} | {row['Command']} | {r_new} | {rps_change} | {l_new} | {lat_change} |"
            )


def main():
    try:
        data = load_data()
    except FileNotFoundError:
        print("\033[91mError: benchmarks.csv not found.\033[0m")
        return

    notes = get_unique_notes(data)
    if not notes:
        print("\033[91mError: CSV is empty or invalid.\033[0m")
        return

    base_note = select_note(notes, "Select BASELINE (The Control Group)")
    target_note = select_note(notes, f"Select TARGET (Comparing against {base_note})")

    print("\033c", end="")

    if target_note == "ALL":
        for note in notes:
            if note != base_note:
                generate_table(data, base_note, note)
    else:
        generate_table(data, base_note, target_note)


if __name__ == "__main__":
    main()

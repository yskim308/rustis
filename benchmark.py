import csv
import datetime
import re
import subprocess
import sys
import time

# --- CONFIGURATION ---
TEST_SUITES = {
    "1": {
        "name": "Quick Sanity Check",
        "desc": "Simple PING to ensure server is alive (No Load)",
        "cmd": ["redis-benchmark", "-t", "ping", "-n", "1000", "-q"],
    },
    "2": {
        "name": "Regular Load (Baseline)",
        "desc": "Standard 50 clients, no pipelining. Good for baseline latency.",
        "cmd": ["redis-benchmark", "-t", "set,get", "-n", "200000", "-q"],
    },
    "3": {
        "name": "High Concurrency & Throughput (Mixed)",
        "desc": "2000 Clients, Pipeline 32, 1 Million Requests.",
        "cmd": [
            "redis-benchmark",
            "-t",
            "set,get,lpush,lpop",
            "-c",
            "2000",  # 2k concurrent connections
            "-P",
            "32",  # Batch 32 commands
            "-n",
            "1000000",  # 1M requests
            "-r",
            "100000",  # Random keys
            "-q",
        ],
    },
    "4": {
        "name": "Heavy Payload Saturation (4KB)",
        "desc": "High Concurrency + 4KB Payloads. Tests bandwidth.",
        "cmd": [
            "redis-benchmark",
            "-t",
            "set,get",
            "-c",
            "1000",
            "-P",
            "16",
            "-d",
            "4096",  # 4KB payload size
            "-n",
            "500000",
            "-r",
            "100000",
            "-q",
        ],
    },
}


def save_to_csv(test_name, output, note):
    """Parses output (RPS and Latency) and saves to benchmarks.csv"""

    # NEW REGEX: Captures both RPS and p50 Latency
    # Matches: "SET: 12345.67 requests per second, p50=1.234 msec"
    regex_pattern = re.compile(
        r"([A-Z]+):\s+([\d\.]+)\s+requests per second,\s+p50=([\d\.]+)\s+msec"
    )

    # Get git hash
    try:
        git_hash = (
            subprocess.check_output(["git", "rev-parse", "--short", "HEAD"])
            .strip()
            .decode("utf-8")
        )
    except:
        git_hash = "unknown"

    timestamp = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")

    rows = []
    found_any = False

    for line in output.splitlines():
        match = regex_pattern.search(line)
        if match:
            found_any = True
            cmd = match.group(1)  # e.g., SET
            rps = match.group(2)  # e.g., 250000.00
            p50 = match.group(3)  # e.g., 2.500

            # Row Structure: Time, Hash, Note, Test Name, Command, RPS, Latency
            rows.append([timestamp, git_hash, note, test_name, cmd, rps, p50])

    if not found_any:
        print(f"  \033[90m[Skipping Save] No valid data found in output.\033[0m")
        return

    filename = "benchmarks.csv"
    file_exists = False
    try:
        with open(filename, "r") as f:
            file_exists = True
    except FileNotFoundError:
        pass

    with open(filename, "a", newline="") as f:
        writer = csv.writer(f)
        # Write header if new file (Updated with Latency column)
        if not file_exists:
            writer.writerow(
                [
                    "Timestamp",
                    "Git Hash",
                    "Note",
                    "Test Name",
                    "Command",
                    "RPS",
                    "Latency (p50)",
                ]
            )
        writer.writerows(rows)

    print(f"  \033[92m✔ Saved to CSV (Note: '{note}')\033[0m")


def print_header():
    print("\033[95m" + "=" * 40)
    print("   RUSTIS BENCHMARK SUITE    ")
    print("=" * 40 + "\033[0m")


def show_menu():
    print_header()
    # Print numbered options
    for key, suite in TEST_SUITES.items():
        print(f"[\033[92m{key}\033[0m] \033[1m{suite['name']}\033[0m")
        print(f"    └── {suite['desc']}")

    # Print special options
    print("-" * 40)
    print("[\033[93ma\033[0m] \033[1mRun ALL Tests\033[0m")
    print("[q] Quit")


def execute_suite(key, batch_label=None):
    """
    Runs a single suite.
    If batch_label is provided, uses it automatically.
    If batch_label is None, prompts the user.
    """
    suite = TEST_SUITES.get(key)
    if not suite:
        return

    print(f"\n\033[93m>>> Running: {suite['name']}...\033[0m")

    try:
        # Run command
        result = subprocess.run(
            suite["cmd"], capture_output=True, text=True, check=False
        )
        print(result.stdout)

        if result.stderr:
            print(f"\033[90m{result.stderr}\033[0m")

        # --- Labeling Logic ---
        note = batch_label

        # If no batch label was passed, ask interactively
        if note is None:
            print("-" * 30)
            user_input = input(
                "\033[36mSave results? Enter label (or press Enter to skip): \033[0m"
            ).strip()
            note = user_input if user_input else ""

        # Only save if we have a label
        if note:
            save_to_csv(suite["name"], result.stdout, note)
        else:
            print("  \033[90m[Skipped Saving]\033[0m")

    except KeyboardInterrupt:
        print("\n\033[91mTest interrupted.\033[0m")
        raise  # Re-raise to stop batch runs if user hits Ctrl+C


def run_all_tests():
    """Runs all suites sequentially with one shared label."""
    print("\n\033[93m>>> BATCH MODE: Running All Tests\033[0m")
    print("This will run every test suite sequentially.")

    label = input(
        "\033[36mEnter label for this entire batch (or press Enter to skip saving): \033[0m"
    ).strip()

    # Iterate through keys 1, 2, 3... in order
    sorted_keys = sorted(TEST_SUITES.keys())

    try:
        for key in sorted_keys:
            execute_suite(key, batch_label=label)
            # Small pause between tests so it doesn't look like a glitch
            time.sleep(1)

        print("\n\033[92m✔ Batch run complete.\033[0m")
        input("Press Enter to return to menu...")

    except KeyboardInterrupt:
        print("\nBatch run aborted.")


def main():
    while True:
        print("\033c", end="")  # Clear screen
        show_menu()
        choice = input("\nSelect > ").strip().lower()

        if choice == "q":
            sys.exit(0)
        elif choice == "a":
            run_all_tests()
        elif choice in TEST_SUITES:
            execute_suite(choice)
            input("\nPress Enter to continue...")
        else:
            pass  # Invalid input, just refresh


if __name__ == "__main__":
    main()

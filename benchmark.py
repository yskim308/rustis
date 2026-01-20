import subprocess
import sys
import time

# --- CONFIGURATION ---
# Define your test suites here.
# You can add as many as you want without writing code.
TEST_SUITES = {
    "1": {
        "name": "Quick Sanity Check",
        "desc": "Simple PING test to ensure server is alive",
        "cmd": ["redis-benchmark", "-t", "ping", "-n", "1000", "-q"],
    },
    "2": {
        "name": "Standard Load (Set/Get)",
        "desc": "100k requests, 50 clients, default payload",
        "cmd": ["redis-benchmark", "-t", "set,get", "-n", "100000", "-q"],
    },
    "3": {
        "name": "High Concurrency (1k Clients)",
        "desc": "Stress testing connection handling",
        "cmd": ["redis-benchmark", "-t", "set,get", "-c", "1000", "-n", "100000", "-q"],
    },
    "4": {
        "name": "Large Payload (4KB)",
        "desc": "Simulating JSON blobs (4KB data)",
        "cmd": ["redis-benchmark", "-t", "set,get", "-d", "4096", "-n", "50000", "-q"],
    },
    "5": {
        "name": "List Operations (Queue Test)",
        "desc": "Testing LPUSH, LPOP, RPOP",
        "cmd": ["redis-benchmark", "-t", "lpush,lpop,rpop", "-n", "100000", "-q"],
    },
    "6": {
        "name": "Pipeline Aggression",
        "desc": "Batching 16 commands at once (High Throughput)",
        "cmd": ["redis-benchmark", "-t", "set,get", "-P", "16", "-q"],
    },
}


def print_header():
    print("\033[95m" + "=" * 40)
    print("   REDIS BENCHMARK SUITE   ")
    print("=" * 40 + "\033[0m")


def show_menu():
    print_header()
    for key, suite in TEST_SUITES.items():
        print(f"[\033[92m{key}\033[0m] \033[1m{suite['name']}\033[0m")
        print(f"    └── {suite['desc']}")
    print("\n[q] Quit")


def run_test(key):
    suite = TEST_SUITES.get(key)
    if not suite:
        print("\n\033[91mInvalid selection\033[0m")
        time.sleep(1)
        return

    print(f"\n\033[93m>>> Running: {suite['name']}...\033[0m")
    print(f"\033[90mCommand: {' '.join(suite['cmd'])}\033[0m\n")

    try:
        # Run the command and stream output
        subprocess.run(suite["cmd"], check=True)
    except KeyboardInterrupt:
        print("\n\033[91mTest interrupted by user.\033[0m")
    except FileNotFoundError:
        print("\n\033[91mError: redis-benchmark not found in PATH.\033[0m")

    input("\nPress Enter to continue...")


def main():
    while True:
        # Clear screen (cross-platform way is messier, this works for Linux/Mac)
        print("\033c", end="")
        show_menu()
        choice = input("\nSelect a test > ").strip().lower()

        if choice == "q":
            print("Exiting.")
            sys.exit(0)

        run_test(choice)


if __name__ == "__main__":
    main()

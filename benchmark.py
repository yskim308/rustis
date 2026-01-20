import subprocess
import sys
import time

# --- CONFIGURATION ---
# Define your test suites here.
# You can add as many as you want without writing code.
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
        "desc": "2000 Clients, Pipeline 32, 1 Million Requests. Tests CPU scaling across GET/SET/LIST.",
        "cmd": [
            "redis-benchmark",
            "-t",
            "set,get,lpush,lpop",
            "-c",
            "2000",  # 2k concurrent connections
            "-P",
            "32",  # Batch 32 commands (High throughput)
            "-n",
            "1000000",  # 1M requests to let it heat up
            "-r",
            "100000",
            "-q",
        ],
    },
    "4": {
        "name": "Heavy Payload Saturation (4KB)",
        "desc": "High Concurrency + 4KB Payloads. Tests memory copying & bandwidth.",
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
            "100000" "-q",
        ],
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
        subprocess.run(suite["cmd"])
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

import subprocess
import time
import os

def measure():
    cmd = [os.path.abspath("dist/echo/echo.exe"), "benchmark"]
    start = time.perf_counter()
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    stdout, stderr = p.communicate()
    elapsed = time.perf_counter() - start
    
    print("--- BENCHMARK STDOUT ---")
    print(stdout.decode("utf-8", errors="replace").strip())
    print(f"Total process wall-clock time (loading + inference + exit): {elapsed:.2f}s")
    print(f"Process return code: {p.returncode}")

if __name__ == "__main__":
    measure()

import os
import sys
import urllib.request
import time

BASE_URL = "https://huggingface.co/futo-org/asr4all-s/resolve/main/"
TARGET_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "models", "asr4all-s")

FILES = [
    ("onnx/metadata.json", "metadata.json"),
    ("onnx/asr4all.onnx.data", "asr4all.onnx.data"),
    ("onnx/stream_c16r4.onnx", "stream_c16r4.onnx"),
    ("onnx/stream_c256r32.onnx", "stream_c256r32.onnx"),
    ("onnx/pcec_c16.onnx", "pcec_c16.onnx"),
    ("onnx/pcec_c16_flush.onnx", "pcec_c16_flush.onnx"),
    ("onnx/vad.onnx", "vad.onnx"),
    ("LICENSE", "LICENSE"),
]

def download_file(src_rel, dest_name):
    url = BASE_URL + src_rel
    dest_path = os.path.join(TARGET_DIR, dest_name)
    
    if os.path.exists(dest_path) and os.path.getsize(dest_path) > 0:
        print(f"[ALREADY EXISTS] {dest_name} ({os.path.getsize(dest_path):,} bytes)")
        return
    
    print(f"[DOWNLOADING] {dest_name} from {url}...")
    start_time = time.time()
    
    def report(count, block_size, total_size):
        downloaded = count * block_size
        pct = (downloaded / total_size * 100) if total_size > 0 else 0
        sys.stdout.write(f"\r  Progress: {pct:.1f}% ({downloaded / 1024 / 1024:.2f} MB / {total_size / 1024 / 1024:.2f} MB)")
        sys.stdout.flush()

    urllib.request.urlretrieve(url, dest_path, reporthook=report)
    print(f"\n  Done in {time.time() - start_time:.2f}s ({os.path.getsize(dest_path):,} bytes)")

def main():
    os.makedirs(TARGET_DIR, exist_ok=True)
    print(f"Target directory: {TARGET_DIR}")
    for src, dst in FILES:
        download_file(src, dst)
    print("\nAll required model files are present.")

if __name__ == "__main__":
    main()

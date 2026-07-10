# collector/run_collector.py
"""schtasks 入口：把 collector 目錄加進 sys.path 後啟動主迴圈。"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from machine_collector.main import main

if __name__ == "__main__":
    main()

# Throwaway environment for the OCR quality spike (P3-01). Not used by the app.
FROM python:3.12-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
        fonts-noto-cjk libgl1 libglib2.0-0 \
    && rm -rf /var/lib/apt/lists/*
RUN pip install --no-cache-dir meikiocr pillow numpy opencv-python-headless
WORKDIR /spike

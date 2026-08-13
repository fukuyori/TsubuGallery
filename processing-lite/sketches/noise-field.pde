// ノイズ場に沿って向きを変える短い線の群れ。
void draw() {
  background(14);

  float s = min(width, height);
  float step = s / 26.0;
  float t = frameCount * 0.006;

  strokeWeight(s * 0.0055);
  for (float y = 0; y < height + step; y += step) {
    for (float x = 0; x < width + step; x += step) {
      float n = fbm(x / s * 3.0 + t, y / s * 3.0 - t * 0.5);
      float angle = n * TWO_PI * 1.5;
      float len = step * (0.55 + n);

      stroke(60 + n * 195, 210 - n * 70, 50 + n * 40, 245);
      line(x, y, x + len * cos(angle), y + len * sin(angle));
    }
  }
}

// オクターブを重ねたノイズ。輪郭が単調になりにくい。
float fbm(float x, float y) {
  float sum = 0;
  float amp = 0.5;
  float freq = 1.0;
  float norm = 0;

  for (int i = 0; i < 3; i++) {
    sum += noise(x * freq, y * freq) * amp;
    norm += amp;
    amp *= 0.5;
    freq *= 2.0;
  }

  return sum / norm;
}

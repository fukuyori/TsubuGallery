// 中心からの距離だけ位相がずれる、脈打つ円のグリッド。
void draw() {
  background(16);

  float s = min(width, height);
  float step = s / 13.0;
  float t = frameCount * 0.045;
  float cx = width * 0.5;
  float cy = height * 0.5;

  noStroke();
  for (float y = step * 0.5; y < height + step; y += step) {
    for (float x = step * 0.5; x < width + step; x += step) {
      float d = dist(x, y, cx, cy);
      float pulse = 0.5 + 0.5 * sin(t - d / step * 0.42);

      fill(255 * (0.25 + 0.75 * pulse), 170 * (0.25 + 0.75 * pulse), 90 * pulse);
      circle(x, y, step * (0.12 + 0.72 * pulse));
    }
  }
}

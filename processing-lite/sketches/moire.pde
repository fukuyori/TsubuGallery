// 2 組の同心円がずれて生まれるモアレ。
void draw() {
  background(250);

  float s = min(width, height);
  float t = frameCount * 0.013;
  float spread = s * 0.11;
  float ox = spread * sin(t);
  float oy = spread * cos(t * 0.73);

  noFill();
  strokeWeight(s * 0.0045);

  stroke(25, 25, 25, 205);
  rings(width * 0.5 - ox, height * 0.5 - oy, s);
  stroke(60, 60, 60, 205);
  rings(width * 0.5 + ox, height * 0.5 + oy, s);

  // ノイズで少し揺らして、機械的になりすぎないようにする。
  float jitter = noise(t, 0) * 0.4 + 0.8;
  stroke(200, 60, 90, 160);
  strokeWeight(s * 0.006 * jitter);
  circle(width * 0.5, height * 0.5, s * 0.22 * jitter);
}

void rings(float cx, float cy, float s) {
  for (int i = 1; i <= 46; i++) {
    circle(cx, cy, i * 1.0 / 46 * s * 1.35);
  }
}

// 位相がゆっくり回るリサージュ曲線を 5 層かさねる。
void draw() {
  background(6);

  float s = min(width, height);
  float t = frameCount * 0.004;

  noFill();
  strokeWeight(s * 0.0035);
  pushMatrix();
  translate(width * 0.5, height * 0.5);

  for (int layer = 0; layer < 5; layer++) {
    float lf = layer * 0.2;
    float amp = s * (0.42 - lf * 0.055);
    float a = 3.0 + lf;
    float b = 4.0 - lf * 0.5;
    float phase = t + lf * 0.6;

    stroke(110 + lf * 145, 90 + lf * 40, 255 - lf * 30, 190);

    float px = amp * sin(phase);
    float py = 0;
    for (int i = 1; i <= 520; i++) {
      float u = i * TWO_PI / 520;
      float x = amp * sin(a * u + phase);
      float y = amp * sin(b * u);
      line(px, py, x, y);
      px = x;
      py = y;
    }
  }

  popMatrix();
}

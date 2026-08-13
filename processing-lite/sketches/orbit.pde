// setup() とグローバル変数の例。角度をフレームまたぎで持ち続ける。
float angle = 0;

void setup() {
  // size() は互換のために受けるが、表示サイズは Viewer が決める。
  size(600, 400);
}

void draw() {
  background(12);

  float s = min(width, height);
  angle += 0.02;

  pushMatrix();
  translate(width * 0.5, height * 0.5);
  noStroke();

  for (int i = 0; i < 60; i++) {
    float a = angle + i * 0.1;
    float r = s * 0.05 + i * s * 0.005;
    fill(255 - i * 3, 120 + i * 2, 60 + i * 3, 220);
    circle(r * cos(a), r * sin(a) * 0.6, s * 0.03 * (1.0 - i * 0.012));
  }

  popMatrix();
}

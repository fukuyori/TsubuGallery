// 黄金角に並べた粒が、全体としてゆっくり回る。
void draw() {
  background(10);

  float s = min(width, height);
  float t = frameCount * 0.008;
  int count = 320;

  noStroke();
  pushMatrix();
  translate(width * 0.5, height * 0.5);
  rotate(t);

  for (int i = 0; i < count; i++) {
    float f = i * 1.0 / count;
    // 黄金角。回しても密度が均一に見える。
    float angle = i * 2.399963;
    float radius = sqrt(f) * s * 0.46;
    float d = map(f, 0, 1, s * 0.030, s * 0.004);

    fill(60 + f * 190, 80 + f * 40, 255 - f * 70, 235);
    circle(radius * cos(angle), radius * sin(angle), d);
  }

  popMatrix();
}

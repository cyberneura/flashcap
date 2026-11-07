import assert from "node:assert/strict";
import {
  classifySensitive,
  findSensitive,
  luhnValid,
  maskRectFor,
  type OcrLine,
} from "../src/lib/sensitiveText.ts";

let failures = 0;
function check(name: string, fn: () => void) {
  try {
    fn();
    console.log(`ok - ${name}`);
  } catch (e) {
    failures++;
    console.log(`not ok - ${name}`);
    console.log(e);
  }
}

const line = (text: string, box: [number, number, number, number] = [0, 0, 10, 10]): OcrLine => ({ text, box });

check("emails", () => {
  assert.equal(classifySensitive("Contact: alice@example.com"), "email");
  assert.equal(classifySensitive("mail me at bob.smith+tag@mail.co.jp today"), "email");
  assert.equal(classifySensitive("@handle on the site"), null);
  assert.equal(classifySensitive("a@b"), null);
  assert.equal(classifySensitive("(mail: alice@example.com)"), "email");
  // recipient lists without spaces
  assert.equal(classifySensitive("alice@example.com,bob@example.com"), "email");
  assert.equal(classifySensitive("To: alice@example.com;bob@example.com"), "email");
  assert.equal(classifySensitive("a@@b.com"), null);
});

check("long lines without a match stay cheap (no quadratic regex)", () => {
  const started = performance.now();
  assert.equal(classifySensitive("a".repeat(100_000)), null);
  assert.equal(classifySensitive("1".repeat(100_000)), null);
  assert.equal(classifySensitive("9 ".repeat(50_000)), null);
  assert.ok(performance.now() - started < 200, "must stay cheap");
});

check("IPv4 addresses", () => {
  assert.equal(classifySensitive("server 192.168.0.12 is down"), "ip");
  assert.equal(classifySensitive("10.0.0.1:8080"), "ip");
  assert.equal(classifySensitive("version 1.2.3.4.5"), null);
  assert.equal(classifySensitive("999.1.1.1"), null);
  assert.equal(classifySensitive("3.14"), null);
});

check("tokens with known prefixes", () => {
  assert.equal(classifySensitive("OPENAI_API_KEY=sk-proj-abcdefghijklmnop"), "token");
  assert.equal(classifySensitive("ghp_abcdefghijklmnopqrstuvwxyz012345"), "token");
  assert.equal(classifySensitive("gho_abcdefghijklmnop"), "token");
  assert.equal(classifySensitive("xoxb-1234567890-abcdefgh"), "token");
  assert.equal(classifySensitive("AKIAIOSFODNN7EXAMPLE"), "token");
  // short tail, or the prefix inside a word, is not a token
  assert.equal(classifySensitive("sk-abc"), null);
  assert.equal(classifySensitive("task-123456789012"), null);
});

check("JWT-like strings", () => {
  const jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
  assert.equal(classifySensitive(`Authorization: Bearer ${jwt}`), "jwt");
  assert.equal(classifySensitive("a.b.c"), null);
  assert.equal(classifySensitive("file.tar.gz"), null);
});

check("long alphanumeric secrets need letters and digits", () => {
  assert.equal(classifySensitive("secret: a1b2c3d4e5f6g7h8i9j0"), "secret");
  assert.equal(classifySensitive("9f8e7d6c5b4a3f2e1d0c"), "secret");
  // plain long words and digit-only ids are not secrets
  assert.equal(classifySensitive("internationalization"), null);
  assert.equal(classifySensitive("order 12345678901234567890 shipped"), null);
  assert.equal(classifySensitive("abcdefghijklmno1"), "secret");
  assert.equal(classifySensitive("abcdefghijklmn1"), null);
});

check("card numbers need Luhn to pass", () => {
  assert.ok(luhnValid("4242424242424242"));
  assert.ok(luhnValid("4111111111111111"));
  assert.ok(!luhnValid("4242424242424241"));
  assert.ok(!luhnValid("123"));
  assert.equal(classifySensitive("Card 4242 4242 4242 4242"), "card");
  assert.equal(classifySensitive("4111-1111-1111-1111"), "card");
  assert.equal(classifySensitive("4242 4242 4242 4241"), null);
  // a stray digit group before the number must not hide it
  assert.equal(classifySensitive("Order 1 4242 4242 4242 4242"), "card");
  assert.equal(classifySensitive("4242 4242 4242 4242 7"), "card");
  // a phone number or a timestamp is not a card
  assert.equal(classifySensitive("tel 090-1234-5678"), null);
  assert.equal(classifySensitive("20261107031500"), null);
});

check("findSensitive keeps only matching lines, in order", () => {
  const lines = [line("hello"), line("x@y.com", [1, 2, 3, 4]), line("plain"), line("10.0.0.1", [5, 6, 7, 8])];
  assert.deepEqual(findSensitive(lines).map((l) => l.text), ["x@y.com", "10.0.0.1"]);
  assert.deepEqual(findSensitive([]), []);
});

check("maskRectFor pads the box and clamps to the image", () => {
  assert.deepEqual(maskRectFor(line("a", [10, 20, 100, 14]), 2, 1000, 1000), { x: 8, y: 18, width: 104, height: 18 });
  assert.deepEqual(maskRectFor(line("a", [0, 0, 100, 14]), 2, 101, 15), { x: 0, y: 0, width: 101, height: 15 });
});

if (failures > 0) {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}

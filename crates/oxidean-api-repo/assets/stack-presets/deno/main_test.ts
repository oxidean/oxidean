import { assertEquals } from "@std/assert";

Deno.test("basic assert", () => {
  assertEquals(1 + 1, 2);
});

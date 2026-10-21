#include <cassert>
#include <iostream>
#include "greet.hpp"

int main() {
  assert(greet("Oxidean") == "Hello, Oxidean!");
  std::cout << "ok\n";
  return 0;
}

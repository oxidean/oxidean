#include <iostream>
#include "greet.hpp"

int main(int argc, char** argv) {
  const char* name = argc > 1 ? argv[1] : "world";
  std::cout << greet(name) << std::endl;
  return 0;
}

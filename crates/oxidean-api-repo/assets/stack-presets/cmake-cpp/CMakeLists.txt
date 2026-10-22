cmake_minimum_required(VERSION 3.16)
project(app LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_executable(app
  src/main.cpp
  src/greet.cpp
)

target_include_directories(app PRIVATE include)

enable_testing()
add_executable(app_tests tests/test_greet.cpp src/greet.cpp)
target_include_directories(app_tests PRIVATE include)
add_test(NAME greet COMMAND app_tests)

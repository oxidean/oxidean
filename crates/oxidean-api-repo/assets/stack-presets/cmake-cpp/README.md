# C / C++ (CMake)

CMake project with library + main + Catch2-style smoke test file.

## Getting started

```bash
cmake -B build
cmake --build build
./build/app
ctest --test-dir build
```

## Docker

```bash
docker build -t app .
docker run --rm app
```

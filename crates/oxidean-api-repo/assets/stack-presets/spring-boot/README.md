# Spring Boot

Maven Spring Boot Web starter (`spring initializr` style).

## Getting started

```bash
./mvnw spring-boot:run
# or
mvn spring-boot:run
```

Open http://localhost:8080

## Tests

```bash
mvn test
```

## Docker

```bash
docker build -t app .
docker run --rm -p 8080:8080 app
```

Open http://localhost:8080

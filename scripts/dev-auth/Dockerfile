# syntax=docker/dockerfile:1
FROM oven/bun:1.4.4-alpine
WORKDIR /app
COPY stubs.mjs ./
ENV PORT=9092
EXPOSE 9092
CMD ["bun", "run", "stubs.mjs"]

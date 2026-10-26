# syntax=docker/dockerfile:1

FROM oven/bun:1-alpine
WORKDIR /app
COPY package.json ./
RUN bun install
COPY . .
ENV PORT=3000
EXPOSE 3000
CMD ["bun", "src/index.ts"]

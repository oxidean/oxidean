# syntax=docker/dockerfile:1

FROM node:26-alpine AS deps
WORKDIR /app
COPY package.json ./
COPY web ./web
RUN npm install

FROM deps AS build
COPY . .
RUN cp -n .env.sample .env || true
RUN npm run build

FROM node:26-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production \
    PORT=3042 \
    PLT_SERVER_HOSTNAME=0.0.0.0 \
    PLT_SERVER_LOGGER_LEVEL=info \
    PLT_MANAGEMENT_API=true
COPY package.json ./
COPY web ./web
RUN npm install --omit=dev
COPY --from=build /app ./
EXPOSE 3042
CMD ["npm", "start"]

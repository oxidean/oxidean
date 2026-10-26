# syntax=docker/dockerfile:1

FROM python:3-slim
WORKDIR /app
COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt
COPY . .
EXPOSE 5000
CMD ["flask", "--app", "wsgi", "run", "--host", "0.0.0.0"]

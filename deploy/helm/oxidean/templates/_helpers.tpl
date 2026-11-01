{{/*
Oxidean chart helpers.
*/}}

{{- define "oxidean.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "oxidean.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := default .Chart.Name .Values.nameOverride -}}
{{- if contains $name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "oxidean.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" -}}
{{- end -}}

{{- define "oxidean.labels" -}}
helm.sh/chart: {{ include "oxidean.chart" . }}
app.kubernetes.io/name: {{ include "oxidean.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: oxidean
{{- end -}}

{{/* Per-component selector labels. */}}
{{- define "oxidean.selectorLabels" -}}
app.kubernetes.io/name: {{ include "oxidean.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "oxidean.componentLabels" -}}
{{ include "oxidean.selectorLabels" . }}
app.kubernetes.io/component: {{ .component }}
{{- end -}}

{{/*
Images — tag defaults: api follows the chart's appVersion (the stamped
oxidean-api release); web and runner fall back to `latest` when unstamped.
*/}}
{{- define "oxidean.image" -}}
{{- $tag := .tag | default .defaultTag -}}
{{- printf "%s:%s" .repository $tag -}}
{{- end -}}

{{- define "oxidean.apiImage" -}}
{{- include "oxidean.image" (dict "repository" .Values.api.image.repository "tag" .Values.api.image.tag "defaultTag" .Chart.AppVersion) -}}
{{- end -}}

{{- define "oxidean.webImage" -}}
{{- include "oxidean.image" (dict "repository" .Values.web.image.repository "tag" .Values.web.image.tag "defaultTag" "latest") -}}
{{- end -}}

{{- define "oxidean.runnerImage" -}}
{{- include "oxidean.image" (dict "repository" .Values.runner.image.repository "tag" .Values.runner.image.tag "defaultTag" "latest") -}}
{{- end -}}

{{/*
The single chart-managed Secret. When secrets.existingSecret is set the chart
renders nothing and pods envFrom the operator's Secret instead.
*/}}
{{- define "oxidean.secretName" -}}
{{- if .Values.secrets.existingSecret -}}
{{- .Values.secrets.existingSecret -}}
{{- else -}}
{{- printf "%s-secrets" (include "oxidean.fullname" .) -}}
{{- end -}}
{{- end -}}

{{- define "oxidean.postgresSecretName" -}}
{{- include "oxidean.secretName" . -}}
{{- end -}}

{{/*
DATABASE_URL resolution:
  1. database.existingSecret → secretKeyRef(database.urlKey)
  2. database.url → the literal value (stored in the chart Secret)
  3. postgres.enabled → rendered to the bundled service
*/}}
{{- define "oxidean.databaseSecretKeyRef" -}}
{{- if .Values.database.existingSecret -}}
valueFrom:
  secretKeyRef:
    name: {{ .Values.database.existingSecret | quote }}
    key: {{ .Values.database.urlKey | quote }}
{{- else -}}
valueFrom:
  secretKeyRef:
    name: {{ include "oxidean.secretName" . | quote }}
    key: DATABASE_URL
{{- end -}}
{{- end -}}

{{/* Rendered bundled-postgres URL — carries POSTGRES_PASSWORD via env expansion. */}}
{{- define "oxidean.bundledDatabaseUrl" -}}
{{- printf "postgres://$(POSTGRES_USER):$(POSTGRES_PASSWORD)@%s-postgres:5432/%s" (include "oxidean.fullname" .) .Values.postgres.database -}}
{{- end -}}

{{/*
Service names.
*/}}
{{- define "oxidean.apiServiceName" -}}
{{- printf "%s-api" (include "oxidean.fullname" .) -}}
{{- end -}}

{{- define "oxidean.webServiceName" -}}
{{- printf "%s-web" (include "oxidean.fullname" .) -}}
{{- end -}}

{{- define "oxidean.sshServiceName" -}}
{{- printf "%s-ssh" (include "oxidean.fullname" .) -}}
{{- end -}}

{{- define "oxidean.postgresServiceName" -}}
{{- printf "%s-postgres" (include "oxidean.fullname" .) -}}
{{- end -}}

{{/*
PVC names.
*/}}
{{- define "oxidean.dataPvcName" -}}
{{- printf "%s-data" (include "oxidean.fullname" .) -}}
{{- end -}}

{{- define "oxidean.reposPvcName" -}}
{{- printf "%s-repos" (include "oxidean.fullname" .) -}}
{{- end -}}

{{- define "oxidean.sshPvcName" -}}
{{- printf "%s-ssh-keys" (include "oxidean.fullname" .) -}}
{{- end -}}

{{- define "oxidean.postgresPvcName" -}}
{{- printf "%s-postgres" (include "oxidean.fullname" .) -}}
{{- end -}}

{{- define "oxidean.runnerPvcName" -}}
{{- printf "%s-runner" (include "oxidean.fullname" .) -}}
{{- end -}}

{{/*
fs vs s3 volume plan (see values.yaml storage.backend):
  fs                          → data PVC at /var (compose parity)
  s3 + retainDataVolume       → identical /var mount; s3 envs inert→live as
                                the api's consumer migration lands
  s3 + !retainDataVolume      → target topology: /var/repos + /var/ssh only
SSH keys always get their own PVC stacked at /var/ssh.
*/}}
{{- define "oxidean.dataVolumeMounted" -}}
{{- if or (ne .Values.storage.backend "s3") .Values.storage.s3.retainDataVolume -}}true{{- end -}}
{{- end -}}

{{/* Advertised SSH endpoint for clone URLs (web meta + api). */}}
{{- define "oxidean.sshHost" -}}
{{- default .Values.ingress.host .Values.ssh.host -}}
{{- end -}}

{{/* storageClass fragment — empty means cluster default. */}}
{{- define "oxidean.storageClass" -}}
{{- if . -}}
storageClassName: {{ . | quote }}
{{- end -}}
{{- end -}}

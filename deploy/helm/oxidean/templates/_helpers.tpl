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

{{/*
Bundled object store (RustFS). When objectstore.enabled and no explicit
storage.s3.endpoint is set, the bundled service is the effective endpoint and
the chart Secret carries the shared credentials both pods read.
*/}}
{{- define "oxidean.objectstoreServiceName" -}}
{{- printf "%s-objectstore" (include "oxidean.fullname" .) -}}
{{- end -}}

{{- define "oxidean.objectstorePvcName" -}}
{{- printf "%s-objectstore" (include "oxidean.fullname" .) -}}
{{- end -}}

{{- define "oxidean.objectstoreEndpoint" -}}
{{- printf "http://%s:9000" (include "oxidean.objectstoreServiceName" .) -}}
{{- end -}}

{{/* True when a bundled store is the effective s3 endpoint (no explicit override). */}}
{{- define "oxidean.s3Bundled" -}}
{{- if and (or .Values.objectstore.enabled .Values.seaweedfs.enabled) (empty .Values.storage.s3.endpoint) -}}true{{- end -}}
{{- end -}}

{{/*
SeaweedFS fullname — replicates the upstream chart's fullname rule so the
api can derive service/secret names (<fullname>-s3, <fullname>-s3-secret).
*/}}
{{- define "oxidean.seaweedfsFullname" -}}
{{- if contains "seaweedfs" .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-seaweedfs" .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "oxidean.seaweedfsS3Endpoint" -}}
{{- printf "http://%s-s3:8333" (include "oxidean.seaweedfsFullname" .) -}}
{{- end -}}

{{/* Effective endpoint — explicit storage.s3.endpoint wins over the bundled service. */}}
{{- define "oxidean.s3Endpoint" -}}
{{- if .Values.storage.s3.endpoint -}}
{{- .Values.storage.s3.endpoint -}}
{{- else if .Values.seaweedfs.enabled -}}
{{- include "oxidean.seaweedfsS3Endpoint" . -}}
{{- else -}}
{{- include "oxidean.objectstoreEndpoint" . -}}
{{- end -}}
{{- end -}}

{{/*
Effective bucket — explicit storage.s3.bucket wins over the bundled default.
objectstore.bucket only applies when the store is enabled, so a bare
storage.backend=s3 still fails closed without a bucket.
*/}}
{{- define "oxidean.s3Bucket" -}}
{{- if or .Values.objectstore.enabled .Values.seaweedfs.enabled -}}
{{- default .Values.objectstore.bucket .Values.storage.s3.bucket -}}
{{- else -}}
{{- .Values.storage.s3.bucket -}}
{{- end -}}
{{- end -}}

{{/*
Shared s3 credentials live in the chart Secret (or secrets.existingSecret)
under the OXIDEAN_S3_* keys — the api reads them directly and a bundled
store either maps them to RUSTFS_* envs or points its s3.credentials.admin
existingSecret at the same keys, so one pair authenticates both sides.

Exception: the zero-config SeaweedFS path. When seaweedfs is the effective
endpoint and no explicit pair or subchart-admin secret is configured, the
subchart generates an admin pair into <fullname>-s3-secret and the api
reads that instead (admin_access_key_id / admin_secret_access_key keys).
*/}}
{{- define "oxidean.s3SeaweedfsGenerated" -}}
{{- if and .Values.seaweedfs.enabled
      (empty .Values.storage.s3.endpoint)
      (empty .Values.storage.s3.accessKeyId)
      (empty .Values.seaweedfs.s3.credentials.admin.existingSecret)
      (empty .Values.seaweedfs.s3.credentials.admin.accessKey) -}}true{{- end -}}
{{- end -}}

{{- define "oxidean.s3CredentialsSecretName" -}}
{{- if eq (include "oxidean.s3SeaweedfsGenerated" .) "true" -}}
{{- printf "%s-s3-secret" (include "oxidean.seaweedfsFullname" .) -}}
{{- else -}}
{{- include "oxidean.secretName" . -}}
{{- end -}}
{{- end -}}

{{- define "oxidean.s3AccessKeyIdKey" -}}
{{- if eq (include "oxidean.s3SeaweedfsGenerated" .) "true" -}}admin_access_key_id{{- else -}}OXIDEAN_S3_ACCESS_KEY_ID{{- end -}}
{{- end -}}

{{- define "oxidean.s3SecretAccessKeyKey" -}}
{{- if eq (include "oxidean.s3SeaweedfsGenerated" .) "true" -}}admin_secret_access_key{{- else -}}OXIDEAN_S3_SECRET_ACCESS_KEY{{- end -}}
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
PVC names — an existingClaim substitutes for the chart-created PVC of the
same role (shared/CSI storage: EFS, Filestore, Azure Files, CephFS,
Longhorn, NFS provisioner). Deployments only ever see the resolved name.
*/}}
{{- define "oxidean.dataPvcName" -}}
{{- default (printf "%s-data" (include "oxidean.fullname" .)) .Values.storage.data.existingClaim -}}
{{- end -}}

{{- define "oxidean.reposPvcName" -}}
{{- default (printf "%s-repos" (include "oxidean.fullname" .)) .Values.storage.s3.repos.existingClaim -}}
{{- end -}}

{{- define "oxidean.sshPvcName" -}}
{{- default (printf "%s-ssh-keys" (include "oxidean.fullname" .)) .Values.ssh.hostKeys.existingClaim -}}
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

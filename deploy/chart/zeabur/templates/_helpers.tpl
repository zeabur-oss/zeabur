{{/* The release's object name: the release name, or "zeabur" when the release has that in it. */}}
{{- define "zeabur.fullname" -}}
{{- if contains .Chart.Name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name .Chart.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "zeabur.selectorLabels" -}}
app.kubernetes.io/name: {{ .Chart.Name }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{/* https://kubernetes.io/docs/concepts/overview/working-with-objects/common-labels/ */}}
{{- define "zeabur.labels" -}}
{{ include "zeabur.selectorLabels" . }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version }}
{{- end -}}

{{/* The public URL's host name, for the Ingress and the HTTPRoute. */}}
{{- define "zeabur.host" -}}
{{- (urlParse (required "publicUrl is required" .Values.publicUrl)).hostname -}}
{{- end -}}

{{- define "zeabur.planKeySecret" -}}
{{- .Values.planKey.existingSecret | default (printf "%s-plan-key" (include "zeabur.fullname" .)) -}}
{{- end -}}

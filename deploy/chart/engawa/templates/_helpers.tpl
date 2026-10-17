{{/* The release's object name: the release name, or "engawa" when the release has that in it. */}}
{{- define "engawa.fullname" -}}
{{- if contains .Chart.Name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name .Chart.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "engawa.selectorLabels" -}}
app.kubernetes.io/name: {{ .Chart.Name }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{/* https://kubernetes.io/docs/concepts/overview/working-with-objects/common-labels/ */}}
{{- define "engawa.labels" -}}
{{ include "engawa.selectorLabels" . }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version }}
{{- end -}}

{{/* The public URL's host name, for the Ingress and the HTTPRoute. */}}
{{- define "engawa.host" -}}
{{- (urlParse (required "publicUrl is required" .Values.publicUrl)).hostname -}}
{{- end -}}

{{- define "engawa.planKeySecret" -}}
{{- .Values.planKey.existingSecret | default (printf "%s-plan-key" (include "engawa.fullname" .)) -}}
{{- end -}}

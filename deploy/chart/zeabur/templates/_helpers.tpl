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
app.kubernetes.io/version: {{ .Chart.Version | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version }}
{{- end -}}

{{/* The sslip.io name of an IP address. sslip.io reads an IPv6 address with each ":" written as "-"
     (https://github.com/cunnie/sslip.io/blob/9f98ba8d30deb426ddddb889c09ed8740e1585fc/xip/xip.go#L763-L766),
     because a DNS label cannot hold ":". */}}
{{- define "zeabur.sslipDomain" -}}
{{- printf "%s.sslip.io" (. | replace ":" "-" | lower) -}}
{{- end -}}

{{/* The domain the addresses are under: global.domain, else <ip>.sslip.io from the first node's
     ExternalIP, else its InternalIP. Read once per render. */}}
{{- define "zeabur.domain" -}}
{{- if not (hasKey .Values.global "_domain") -}}
{{- $domain := .Values.global.domain -}}
{{- if not $domain -}}
{{- $ip := "" -}}
{{- range $node := (lookup "v1" "Node" "" "").items | default list -}}
{{- if not $ip -}}
{{- range $node.status.addresses -}}{{- if and (eq .type "ExternalIP") (not $ip) -}}{{- $ip = .address -}}{{- end -}}{{- end -}}
{{- range $node.status.addresses -}}{{- if and (eq .type "InternalIP") (not $ip) -}}{{- $ip = .address -}}{{- end -}}{{- end -}}
{{- end -}}
{{- end -}}
{{- $domain = include "zeabur.sslipDomain" (required "global.domain is required when the chart cannot read the cluster's nodes (helm template)" $ip) -}}
{{- end -}}
{{- if not (regexMatch "^[a-z0-9]([-a-z0-9]{0,61}[a-z0-9])?(\\.[a-z0-9]([-a-z0-9]{0,61}[a-z0-9])?)*$" $domain) -}}
{{- fail (printf "global.domain must be a lowercase domain name, not %q" $domain) -}}
{{- end -}}
{{- $_ := set .Values.global "_domain" $domain -}}
{{- end -}}
{{- .Values.global._domain -}}
{{- end -}}

{{/* publicUrl, else https://zeabur.<domain>. */}}
{{- define "zeabur.publicUrl" -}}
{{- if .Values.publicUrl -}}
{{- .Values.publicUrl -}}
{{- else -}}
https://zeabur.{{ include "zeabur.domain" . }}
{{- end -}}
{{- end -}}

{{/* issuer.url, else the Zeabur Identity this chart installs. */}}
{{- define "zeabur.issuerUrl" -}}
{{- if .Values.issuer.url -}}
{{- .Values.issuer.url -}}
{{- else if .Values.identity.issuer -}}
{{- .Values.identity.issuer -}}
{{- else if .Values.identity.enabled -}}
https://identity.{{ include "zeabur.domain" . }}
{{- else -}}
{{- fail "issuer.url is required when identity.enabled is false" -}}
{{- end -}}
{{- end -}}

{{/* The chart makes the client secret: it installs Identity and no Secret is given. */}}
{{- define "zeabur.ownClientSecret" -}}
{{- if and .Values.identity.enabled (not .Values.issuer.clientSecret.existingSecret) -}}true{{- end -}}
{{- end -}}

{{- define "zeabur.clientSecret" -}}
{{- if include "zeabur.ownClientSecret" . -}}
{{- printf "%s-client" (include "zeabur.fullname" .) -}}
{{- else -}}
{{- required "issuer.clientSecret.existingSecret is required when identity.enabled is false" .Values.issuer.clientSecret.existingSecret -}}
{{- end -}}
{{- end -}}

{{/* The domains new URLs are suggested under: domains.suffixes, else an sslip.io domain. */}}
{{- define "zeabur.domainSuffixes" -}}
{{- if .Values.domains.suffixes -}}
{{- join "," .Values.domains.suffixes -}}
{{- else if or (not .Values.global.domain) (hasSuffix ".sslip.io" .Values.global.domain) -}}
{{- include "zeabur.domain" . -}}
{{- end -}}
{{- end -}}

{{/* The public URL's host name, for the Ingress and the HTTPRoute. */}}
{{- define "zeabur.host" -}}
{{- (urlParse (include "zeabur.publicUrl" .)).hostname -}}
{{- end -}}

{{- define "zeabur.image" -}}
{{- if .Values.image.digest -}}
{{ .Values.image.repository }}@{{ .Values.image.digest }}
{{- else -}}
{{ .Values.image.repository }}:{{ .Values.image.tag | default .Chart.Version }}
{{- end -}}
{{- end -}}

{{- define "zeabur.installationIdConfigMap" -}}
{{- printf "%s-installation-id" (include "zeabur.fullname" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "zeabur.planKeySecret" -}}
{{- .Values.planKey.existingSecret | default (printf "%s-plan-key" (include "zeabur.fullname" .)) -}}
{{- end -}}

{{/* Quantities by resource as `cpu=10m,memory=64Mi` (ZEABUR_TOOLS_REQUESTS, ZEABUR_TOOLS_LIMITS). */}}
{{- define "zeabur.quantities" -}}
{{- $pairs := list -}}
{{- range $name, $quantity := . -}}
{{- $pairs = append $pairs (printf "%s=%v" $name $quantity) -}}
{{- end -}}
{{- join "," $pairs -}}
{{- end -}}

{{/* Zeabur Proxy's objects. Its pods carry `app.kubernetes.io/name: zeabur-proxy`, which Zeabur
     Catalog's NetworkPolicy for VictoriaLogs admits from the namespace `zeabur`. */}}
{{- define "zeabur.proxyName" -}}
{{- printf "%s-proxy" (include "zeabur.fullname" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "zeabur.proxySelectorLabels" -}}
app.kubernetes.io/name: zeabur-proxy
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "zeabur.proxyLabels" -}}
{{ include "zeabur.proxySelectorLabels" . }}
app.kubernetes.io/version: {{ .Chart.Version | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version }}
{{- end -}}

{{- define "zeabur.proxyTlsSecret" -}}
{{- .Values.proxy.tls.existingSecret | default (printf "%s-tls" (include "zeabur.proxyName" .)) -}}
{{- end -}}

{{/* Keep the suffix even when the release has a long name. */}}
{{- define "zeabur.databaseUpdaterName" -}}
{{- printf "%s-database-updater" (include "zeabur.fullname" . | trunc 46 | trimSuffix "-") -}}
{{- end -}}

{{/* How Zeabur reaches out (extraTrustedCA, outboundProxy), the same in the server and the
     components Job: env entries, the extra-ca volume and its mount. */}}
{{- define "zeabur.outboundEnv" -}}
{{- if or .Values.extraTrustedCA.configMap .Values.extraTrustedCA.secret }}
- name: ZEABUR_EXTRA_CA_FILE
  value: /etc/zeabur/extra-ca/{{ .Values.extraTrustedCA.key }}
{{- end }}
{{- with .Values.outboundProxy }}
{{- if .enabled }}
{{- if .httpProxy }}
- name: HTTP_PROXY
  value: {{ .httpProxy | quote }}
{{- end }}
{{- if .httpsProxy }}
- name: HTTPS_PROXY
  value: {{ .httpsProxy | quote }}
{{- end }}
- name: NO_PROXY
  value: {{ concat (list "localhost" ".svc" ".cluster.local") .clusterNetwork .noProxy | join "," | quote }}
{{- end }}
{{- end }}
{{- end -}}

{{- define "zeabur.extraCaMount" -}}
{{- if or .Values.extraTrustedCA.configMap .Values.extraTrustedCA.secret }}
- name: extra-ca
  mountPath: /etc/zeabur/extra-ca
  readOnly: true
{{- end }}
{{- end -}}

{{- define "zeabur.extraCaVolume" -}}
{{- with .Values.extraTrustedCA }}
{{- if .configMap }}
- name: extra-ca
  configMap:
    name: {{ .configMap }}
    items:
      - key: {{ .key }}
        path: {{ .key }}
{{- else if .secret }}
- name: extra-ca
  secret:
    secretName: {{ .secret }}
    defaultMode: 0440
    items:
      - key: {{ .key }}
        path: {{ .key }}
{{- end }}
{{- end }}
{{- end -}}

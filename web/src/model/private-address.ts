// A Service for a workload that has none, as `kubectl expose` makes one: named like the workload,
// selecting its pods by its `matchLabels`, with every port its containers declare.
import type { Schemas } from "@/api/client";
import { toYaml } from "@/lib/yaml-out";
import type { Service } from "@/model/service";

/** The port in {@link exposeCommand} when the port is not known. */
export const PORT_PLACEHOLDER = "<port>";

export type Exposure =
  | { can: true; selector: Record<string, string> }
  /** `kind`: a Job or CronJob. `expressions`: its selector has `matchExpressions`. */
  | { can: false; why: "kind" | "expressions" };

export function exposureOf(service: Service): Exposure {
  const selector = service.selector;
  if (!selector) return { can: false, why: "kind" };
  if (selector.expressions || Object.keys(selector.matchLabels).length === 0)
    return { can: false, why: "expressions" };
  return { can: true, selector: selector.matchLabels };
}

const running = (service: Service) => service.containers.filter((c) => !c.init);

/** The ports the containers declare, once each. */
export function declaredPorts(service: Service): Schemas.ContainerPort[] {
  const seen = new Set<string>();
  return running(service)
    .flatMap((c) => c.ports ?? [])
    .filter((p) => {
      const key = `${p.port}/${p.protocol ?? "TCP"}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
}

/** The port the app likely answers on: its readiness probe's, else the first TCP port it
 * declares. */
export function likelyPort(service: Service): number | undefined {
  for (const container of running(service)) {
    const probe = container.probes?.readiness?.port;
    if (!probe) continue;
    if (/^\d+$/.test(probe)) return Number(probe);
    const named = container.ports?.find((p) => p.name === probe);
    if (named) return named.port;
  }
  return declaredPorts(service).find((p) => !p.protocol)?.port;
}

export const isPort = (port: number) =>
  Number.isInteger(port) && port >= 1 && port <= 65535;

/** `port` first, then every other port the containers declare. */
function portsOf(service: Service, port: number) {
  const declared = declaredPorts(service);
  const first = declared.find((p) => p.port === port && !p.protocol) ?? {
    port,
  };
  const all = [first, ...declared.filter((p) => p !== first)];
  const taken = new Set(all.flatMap((p) => p.name ?? []));
  const nameOf = (p: Schemas.ContainerPort) => {
    if (p.name) return p.name;
    const base = `${p.protocol?.toLowerCase() ?? "port"}-${p.port}`;
    let name = base;
    for (let n = 2; taken.has(name); n++) name = `${base}-${n}`;
    taken.add(name);
    return name;
  };
  return all.map((p) => ({
    ...(all.length > 1 ? { name: nameOf(p) } : {}),
    port: p.port,
    targetPort: p.port,
    ...(p.protocol ? { protocol: p.protocol } : {}),
  }));
}

/** The workload's name as a Service name, which must be a DNS label starting with a letter:
 * `api.worker` becomes `api-worker`, and a name past 63 characters is cut. */
export function serviceNameOf(service: Service): string {
  const name = service.name.replaceAll(".", "-");
  const lettered = /^[a-z]/.test(name) ? name : `svc-${name}`;
  return lettered.slice(0, 63).replace(/-+$/, "");
}

export function privateService(
  service: Service,
  selector: Record<string, string>,
  port: number,
) {
  // Deleting the workload takes the Service only with this label, and a label value has the
  // same 63-character limit.
  const labels =
    service.name.length <= 63
      ? { labels: { "app.kubernetes.io/name": service.name } }
      : {};
  return {
    apiVersion: "v1",
    kind: "Service",
    metadata: { name: serviceNameOf(service), ...labels },
    spec: { type: "ClusterIP", selector, ports: portsOf(service, port) },
  };
}

export const privateAddress = (service: Service, port: number) =>
  `${serviceNameOf(service)}.${service.namespace}.svc:${port}`;

const named = (service: Service) => {
  const name = serviceNameOf(service);
  return name === service.name ? "" : ` --name=${name}`;
};

/** The kubectl command that makes the same Service: `kubectl expose` for a Deployment and one
 * port, as it cannot expose a StatefulSet or DaemonSet, else the Service applied as YAML. */
export function exposeCommand(
  service: Service,
  selector: Record<string, string>,
  port?: number,
): string {
  const ns = `kubectl -n ${service.namespace}`;
  const single = declaredPorts(service).every(
    (p) => p.port === port && !p.protocol,
  );
  if (service.kind === "Deployment" && single)
    return `${ns} expose deployment ${service.name}${named(service)} --port=${port ?? PORT_PLACEHOLDER}`;
  const object = privateService(
    service,
    selector,
    port ?? (PORT_PLACEHOLDER as never),
  );
  const yaml = toYaml([object]).replaceAll(
    JSON.stringify(PORT_PLACEHOLDER),
    PORT_PLACEHOLDER,
  );
  return `${ns} apply -f - <<'EOF'\n${yaml}EOF`;
}

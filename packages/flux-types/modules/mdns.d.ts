declare module "flux:mdns" {
  /** Options common to {@link resolve}, {@link browse}, and {@link services}. */
  type MdnsOptions = {
    /** How long (ms) to collect multicast answers before resolving. Default 1500. */
    timeoutMs?: number
  }

  /** A reverse-resolved address, from {@link resolve}. */
  type Resolved = {
    /** The queried IPv4 address. */
    ip: string
    /** Its mDNS hostname, e.g. "printer.local". */
    host: string
  }

  /** One discovered DNS-SD service instance, from {@link browse}. */
  type ServiceInstance = {
    /** The human instance label, e.g. "Office Printer". */
    instance: string
    /** The service type, e.g. "_ipp._tcp". */
    service: string
    /** The target host the SRV record points at, e.g. "printer.local". */
    host: string
    /** The advertised port. */
    port: number
    /** A/AAAA addresses for `host`, when the responder bundled them. */
    addrs: string[]
    /** TXT attributes (a bare flag attribute has an empty-string value). */
    txt: Record<string, string>
  }

  /**
   * Reverse-resolve IPv4 addresses to their mDNS `.local` hostnames over the
   * link-local multicast group (a PTR query against `in-addr.arpa`). `.local`
   * names are mDNS, not unicast DNS, so this works with no `nss-mdns` resolver and
   * no external binary. Addresses that do not answer within the window — and any
   * IPv6 inputs — are simply absent from the result; an empty input resolves to
   * `[]` without touching the network. Needs a Bonjour/avahi responder on the LAN.
   */
  export function resolve(ips: string[], opts?: MdnsOptions): Promise<Resolved[]>

  /**
   * Browse a DNS-SD service type for the instances on the LAN. `service` may be
   * bare (`"_http._tcp"`) or fully qualified. Resolves to `[]` if nothing answers
   * within the window.
   */
  export function browse(service: string, opts?: MdnsOptions): Promise<ServiceInstance[]>

  /**
   * Enumerate the service types advertised on the LAN (the
   * `_services._dns-sd._udp.local` meta-query), e.g. `["_http._tcp", "_ipp._tcp"]`.
   */
  export function services(opts?: MdnsOptions): Promise<string[]>
}
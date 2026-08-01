// Dev-machine workaround: github.com is SNI-reset on this network, but the
// GitHub IP 20.27.177.113 is reachable. Patch dns.lookup so Node's fetch can
// connect. Used only for running `tauri-plugin-libmpv-api setup-lib` via:
//   node --import ../dns-patch.mjs path/to/cli.cjs setup-lib
import dns from 'node:dns'

const GITHUB_IP = '20.27.177.113'
const origLookup = dns.lookup

dns.lookup = function patchedLookup(hostname, options, callback) {
  if (typeof options === 'function') {
    callback = options
    options = {}
  }
  if (hostname === 'github.com') {
    if (options && options.all) {
      return callback(null, [{ address: GITHUB_IP, family: 4 }])
    }
    return callback(null, GITHUB_IP, 4)
  }
  return origLookup.call(dns, hostname, options, callback)
}

console.log('[dns-patch] github.com ->', GITHUB_IP)

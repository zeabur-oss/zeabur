// The end of "Retry as admin", in the window engawa opened for it: tell engawa's page how it went,
// then close. A BroadcastChannel reaches every page of this origin, also when the window lost its
// opener: engawa's page sets Cross-Origin-Opener-Policy: same-origin, so the trip through the
// issuer severs window.opener.
const { result, until } = document.documentElement.dataset
const channel = new BroadcastChannel("engawa-elevation")
channel.postMessage({ result, until })
channel.close()
window.close()

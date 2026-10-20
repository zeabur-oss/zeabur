// The end of "Retry as admin", in the window Zeabur opened for it: tell Zeabur's page how it went,
// then close. A BroadcastChannel reaches every page of this origin, also when the window lost its
// opener: Zeabur's page sets Cross-Origin-Opener-Policy: same-origin, so the trip through the
// issuer severs window.opener.
const { result, until } = document.documentElement.dataset
const channel = new BroadcastChannel("zeabur-elevation")
channel.postMessage({ result, until })
channel.close()
window.close()

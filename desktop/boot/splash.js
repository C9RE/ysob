// No handshake in the pack: show the dots and the wordmark instead.
const shake = document.querySelector('.shake');
const bare = () => document.body.classList.add('bare');
shake.addEventListener('error', bare);
if (shake.complete && !shake.naturalWidth) bare();

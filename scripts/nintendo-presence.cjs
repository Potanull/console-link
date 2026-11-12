const {execFile} = require('node:child_process');
const path = require('node:path');
function clean(value, limit) {
  if (typeof value !== 'string') return '';
  return [...value].filter(c => !/[\x00-\x1f\x7f-\x9f]/.test(c)).slice(0, limit).join('').trim();
}
function coverUrl(value) {
  if (typeof value !== 'string' || value.length > 2048) return null;
  try { const u = new URL(value); return u.protocol === 'https:' && !u.username && !u.password && !u.port && ['atum-img-lp1.cdn.nintendo.net','img-eshop.cdn.nintendo.net'].includes(u.hostname) ? value : null; } catch { return null; }
}
function filterFriends(input) {
  if (!Array.isArray(input) || input.length > 300) throw new Error('Invalid friends');
  return input.map(f => {
    if (!/^[0-9a-f]{16}$/i.test(f.nsaId) || !['ONLINE','PLAYING','INACTIVE','OFFLINE'].includes(f.presence?.state)) throw new Error('Invalid presence');
    const online = ['ONLINE','PLAYING'].includes(f.presence.state);
    return {id:f.nsaId, name:clean(f.name,128), state:f.presence.state,
      game: online ? clean(f.presence.game?.name,128) || null : null,
      cover_url: online ? coverUrl(f.presence.game?.imageUri) : null};
  });
}
function main(base) {
  const entry = path.join(base, 'node_modules/nxapi/bin/nxapi.js');
  // Credentials and raw SDK responses never cross the helper protocol boundary.
  execFile(process.execPath, [entry, '--data-path', path.join(base,'data'), 'nso','friends','--json'], {
    timeout:40000, maxBuffer:2*1024*1024, windowsHide:true,
    env:{...process.env, NXAPI_DEBUG_FILE:'0', DEBUG:'', NXAPI_SKIP_UPDATE_CHECK:'1',
      NXAPI_USER_AGENT:'ConsoleLink/0.6.0 (Nintendo presence integration)'}
  }, (error,stdout) => {
    let result = {ok:false};
    if (!error) {
      try { result = {ok:true,friends:filterFriends(JSON.parse(stdout))}; } catch {}
    }
    process.stdout.write(JSON.stringify(result));
  });
}
module.exports = {filterFriends};
if (require.main === module || process.argv[1] === '--console-link-presence') main(process.argv[2]);

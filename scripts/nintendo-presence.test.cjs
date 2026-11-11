const {test} = require('node:test');
const assert = require('node:assert/strict');
const {filterFriends} = require('./nintendo-presence.cjs');
test('only allowlisted fields leave nxapi', () => {
 const raw = [{nsaId:'0123456789abcdef',name:'Test Player',secret:'DO_NOT_EXPORT',presence:{state:'ONLINE',game:{name:'Test Game',token:'DO_NOT_EXPORT'}}}];
 assert.deepEqual(filterFriends(raw),[{id:'0123456789abcdef',name:'Test Player',state:'ONLINE',game:'Test Game',cover_url:null}]);
 assert(!JSON.stringify(filterFriends(raw)).includes('DO_NOT_EXPORT'));
});
test('offline never exposes the previous game', () => {
 assert.equal(filterFriends([{nsaId:'0123456789abcdef',name:'Test',presence:{state:'INACTIVE',game:{name:'Old Game'}}}])[0].game,null);
});
test('malformed or excessively large replies rejected', () => {
 assert.throws(()=>filterFriends({token:'DO_NOT_EXPORT'}));
 assert.throws(()=>filterFriends(new Array(301).fill({})));
 assert.throws(()=>filterFriends([{nsaId:'bad',presence:{state:'ONLINE'}}]));
});
test('Nintendo artwork is filtered to public CDN URLs', () => {
 const make = imageUri => [{nsaId:'0123456789abcdef',name:'Test',presence:{state:'ONLINE',game:{name:'Test Game',imageUri}}}];
 const url = 'https://atum-img-lp1.cdn.nintendo.net/example.jpg';
 assert.equal(filterFriends(make(url))[0].cover_url,url);
 for(const unsafe of ['http://atum-img-lp1.cdn.nintendo.net/a','https://localhost/a','https://user:pass@atum-img-lp1.cdn.nintendo.net/a']) assert.equal(filterFriends(make(unsafe))[0].cover_url,null);
});

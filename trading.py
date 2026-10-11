"""Owner-authorized town offers, revalidated and exchanged atomically by the host."""
from copy import deepcopy
import secrets

import progression as economy


def market(town):
    return town['level'] >= 5 and any(b['kind'] == 'market' and 'construction' not in b for b in town['buildings'])


def terms(data, prefix):
    resource, amount = data.get(prefix + '_resource'), data.get(prefix + '_amount')
    if resource not in economy.TRADABLE_GOODS or type(amount) is not int or not 1 <= amount <= 100:
        raise ValueError('Choose trade goods and a whole quantity from 1 to 100.')
    return resource, amount


def receive(town, key, amount):
    if key == 'coins':
        town['coins'] += amount
    else:
        if economy.storage_room(town, key) + 1e-9 < amount:
            raise ValueError('Both towns need enough enabled storage for the incoming goods.')
        economy.deliver(town, key, amount, float('inf'), record=False)


def change(town, data, world):
    if not world:
        raise ValueError('Town trading requires a shared world.')
    town_id = next((key for key, value in world['towns'].items() if value is town), None)
    if town_id is None:
        raise ValueError('Choose your own town.')
    if not market(town):
        raise ValueError('Finish a market stall at hall level 5 first.')
    action = data['action']
    if action == 'create_trade':
        target_id = data.get('target_town')
        if not isinstance(target_id, str) or target_id == town_id or target_id not in world['towns']:
            raise ValueError('Choose another town.')
        if not market(world['towns'][target_id]):
            raise ValueError('The other town needs a completed market stall at hall level 5.')
        give, give_amount = terms(data, 'give')
        want, want_amount = terms(data, 'want')
        if give == want:
            raise ValueError('Choose different goods on each side of the trade.')
        if not economy.afford(town, {give: give_amount}):
            raise ValueError('Not enough goods to offer.')
        offers = town.get('trade_offers', [])
        if len(offers) >= 10:
            raise ValueError('Cancel an offer before creating more (maximum 10).')
        town.setdefault('trade_offers', []).append({'id': secrets.token_hex(8), 'target_town': target_id,
                                                  'give_resource': give, 'give_amount': give_amount,
                                                  'want_resource': want, 'want_amount': want_amount})
        return {}
    if action == 'cancel_trade':
        offers = town.get('trade_offers', [])
        offer = next((o for o in offers if o['id'] == data.get('offer_id')), None)
        if offer is None:
            raise ValueError('That offer is no longer available.')
        offers.remove(offer)
        return {}
    source_id = data.get('source_town')
    source = world['towns'].get(source_id) if isinstance(source_id, str) else None
    offer = next((o for o in source.get('trade_offers', []) if o['id'] == data.get('offer_id')), None) if source else None
    if source is town or offer is None or offer['target_town'] != town_id or not market(source):
        raise ValueError('That trade offer is not available to your town.')
    give, give_amount = terms(offer, 'give')
    want, want_amount = terms(offer, 'want')
    if not economy.afford(source, {give: give_amount}) or not economy.afford(town, {want: want_amount}):
        raise ValueError('Both towns must still have the offered goods.')
    # Simulate outgoing stock first, including the storage it frees, before changing either town.
    for original, outgoing, outgoing_amount, incoming, incoming_amount in (
            (source, give, give_amount, want, want_amount), (town, want, want_amount, give, give_amount)):
        trial = {**original, 'buildings': deepcopy(original['buildings']), 'goods': dict(original.get('goods', {}))}
        economy.spend(trial, {outgoing: outgoing_amount})
        receive(trial, incoming, incoming_amount)
    economy.spend(source, {give: give_amount})
    economy.spend(town, {want: want_amount})
    receive(source, want, want_amount)
    receive(town, give, give_amount)
    source['trade_offers'].remove(offer)
    for participant in (source, town):
        participant['completed_trades'] = participant.get('completed_trades', 0) + 1
    return {}

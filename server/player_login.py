"""Private, per-world player credentials; public towns never contain PINs."""
import hashlib
import secrets


def credentials(name, pin):
    if not isinstance(name, str) or not 1 <= len(name.strip()) <= 24:
        raise ValueError('Enter a player name of 1–24 characters.')
    if not isinstance(pin, str) or len(pin) != 4 or not pin.isascii() or not pin.isdigit():
        raise ValueError('Enter a four-digit PIN.')
    return name.strip().casefold(), pin


def account(name, pin, town_id):
    salt = secrets.token_hex(16)
    return {'name': name.strip(), 'town_id': town_id, 'salt': salt,
            'pin_hash': digest(pin, salt)}


def digest(pin, salt):
    return hashlib.pbkdf2_hmac('sha256', pin.encode(), bytes.fromhex(salt), 200000).hex()


def login(entry, data, token, create_town):
    name = data.get('player_name')
    key, pin = credentials(name, data.get('pin'))
    accounts = entry.setdefault('players', {})
    existing = accounts.get(key)
    if existing:
        if not secrets.compare_digest(existing['pin_hash'], digest(pin, existing['salt'])):
            raise ValueError('Incorrect player name or PIN.')
        town_id = existing['town_id']
        if town_id not in entry['world']['towns']:
            raise ValueError('Your village no longer exists.')
    else:
        town_id = entry['tokens'].get(token)
        if town_id not in entry['world']['towns']:
            town_id = None
        if town_id and any(a['town_id'] == town_id for a in accounts.values()):
            raise ValueError('This village already has a player name. Use its name and PIN.')
        if not town_id:
            town_name = data.get('town_name')
            if town_name is None:
                return {'needs_town': True}
            if not isinstance(town_name, str) or not 1 <= len(town_name.strip()) <= 24:
                raise ValueError('Enter a town name of 1–24 characters.')
            if len(entry['world']['towns']) >= 4:
                raise ValueError('This world already has four towns.')
            town_id = secrets.token_hex(8)
            entry['world']['towns'][town_id] = create_town(town_name.strip())
        accounts[key] = account(name, pin, town_id)
    # Reuse the town's bearer token so reconnecting does not grow the save.
    token = next((t for t, owner in entry['tokens'].items() if owner == town_id), None)
    if token is None:
        token = secrets.token_urlsafe(24)
        entry['tokens'][token] = town_id
    return {'token': token, 'town_id': town_id}

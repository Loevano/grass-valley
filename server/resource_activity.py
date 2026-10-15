"""Host-recorded resource flows, grouped by simulation time in a bounded minute."""


def advance(town, seconds):
    history = town.setdefault('resource_activity', {'seconds': 0, 'buckets': []})
    history['seconds'] += seconds
    cutoff = history['seconds'] - 60
    history['buckets'] = [bucket for bucket in history['buckets'] if bucket['at'] > cutoff]


def record(town, key, amount, direction, food_value=0):
    if amount <= 1e-9 or town.get('_local_production') or town.get('_activity_preview'):
        return
    history = town.setdefault('resource_activity', {'seconds': 0, 'buckets': []})
    if not history['buckets'] or history['buckets'][-1]['at'] != history['seconds']:
        history['buckets'].append({'at': history['seconds'], 'in': {}, 'out': {}})
    values = history['buckets'][-1][direction]
    values[key] = values.get(key, 0) + amount
    if food_value:
        values['food'] = values.get('food', 0) + amount * food_value

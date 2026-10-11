"""Wild horse capture, stable breeding stock, and the founder's riding horse."""
import progression as economy


def stable(town, data):
    index = data.get('index')
    if type(index) is not int or not 0 <= index < len(town['buildings']):
        raise ValueError('Choose a stable.')
    building = town['buildings'][index]
    if building['kind'] != 'stable' or 'construction' in building or town['level'] < 8:
        raise ValueError('Finish a stable at hall level 8 first.')
    if economy.distance(town['player'], building) > 1.8:
        raise ValueError('Walk beside your stable first.')
    return building


def add_horse(building, age):
    count = building.get('animals', {}).get('horse', 0)
    if count >= 4 * building.get('level', 1):
        raise ValueError('Stable full. Upgrade it for more horse spaces.')
    ages = economy.animal_growth(building, 'horse') + [age]
    building.setdefault('animals', {})['horse'] = count + 1
    building.setdefault('animal_growth', {})['horse'] = ages


def change(town, data):
    player = town['player']
    action = data['action']
    if action == 'lead_horse':
        if town['level'] < 8 or not any(b['kind'] == 'stable' and 'construction' not in b for b in town['buildings']):
            raise ValueError('Finish a stable at hall level 8 before capturing horses.')
        if player.get('leading_horse'):
            raise ValueError('Bring the horse you are leading to your stable first.')
        horse = next((r for r in town['map_resources'] if r['kind'] == 'wild_horse'
                      and r['x'] == data.get('x') and r['y'] == data.get('y')), None)
        if horse is None or economy.distance(player, horse) > 1.8:
            raise ValueError('Walk beside a wild horse first.')
        town['map_resources'].remove(horse)
        player['leading_horse'] = True
        return {}
    if action == 'toggle_mount':
        if not player.get('horse'):
            raise ValueError('Saddle a settled horse at your stable first.')
        player['mounted'] = not player.get('mounted', False)
        player.pop('activity', None)
        return {'player': dict(player)}
    building = stable(town, data)
    if action == 'stable_horse':
        if not player.get('leading_horse'):
            raise ValueError('Lead a wild horse to the stable first.')
        add_horse(building, 0)
        player.pop('leading_horse')
    elif action == 'return_horse':
        if not player.get('horse'):
            raise ValueError('You do not have a riding horse.')
        add_horse(building, economy.ANIMAL_GROWTH_SECONDS)
        player.pop('horse')
        player.pop('mounted', None)
    elif action == 'ride_horse':
        if player.get('horse'):
            raise ValueError('Return your current riding horse first.')
        ages = economy.animal_growth(building, 'horse')
        if not any(age >= economy.ANIMAL_GROWTH_SECONDS for age in ages):
            raise ValueError('Feed a horse for five minutes to settle it first.')
        costs = {} if player.get('saddle') else {'saddles': 1}
        if not economy.afford(town, costs):
            raise ValueError('Craft a saddle at the saddler first.')
        economy.spend(town, costs)
        ages.remove(next(age for age in ages if age >= economy.ANIMAL_GROWTH_SECONDS))
        building['animals']['horse'] -= 1
        building.setdefault('animal_growth', {})['horse'] = ages
        player.update(horse=True, mounted=True, saddle=True)
        player.pop('activity', None)
        town['horse_ridden'] = True
    return {}

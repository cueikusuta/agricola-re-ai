with open('src/cards.rs', 'r') as f:
    content = f.read()
if '<<<<<<<' in content:
    print('CONFLICTS REMAIN')
else:
    print('cards.rs: OK - no conflict markers')
    # Verify key variants exist
    for variant in ['AfterImprovementPlayedGain', 'AfterOccupationGain', 'AfterAnyWorkerGain', 'AfterAnyWorkerOthersGain', 'HarvestStartGainIfFields', 'OnPlayGainAnimal', 'AfterWorkerGainAnimal']:
        if variant in content:
            print(f'  {variant}: FOUND')
        else:
            print(f'  {variant}: MISSING!')
(* validation: feature=diagnostics.limits clause=limits *)
PROGRAM SourceSizeLimit
VAR
    ThisIdentifierIsIntentionallyTooLongForTheSmallValidationProfileAndKeepsGoingPastTheDefaultOneHundredTwentyEightCharacterLimitForTheCompilerReadinessCorpus : INT := 0;
END_VAR
END_PROGRAM

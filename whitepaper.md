Outsourcing decision making for prediction markets
-------------

# Abstract
In traditional prediction markets decisions are usually made by the plattforms themselves. This is not ideal because the platform needs to be trusted to make rational outcome decisions. To circumvent this and make it possible to create prediction markets without trusted third parties we propose a mechanism where users themselves act as judges who vote on outcomes and are incentivized through a dedicated judge share of prediction market funds. This makes it possible to incentivize users to act as the oracle for prediction outcomes. 


# Motivations

# Definitions

## Prediction
A well formulated prediction where the outcome of the prediction needs to become clearly evident at some point in time of the future. 

## Users
Entities which can use the prediction market platform. 

## Bets
A wager of a user on a predicted outcome of a prediction. This can be true or false and has a size which is measured in Bitcoin/satoshis.

## Judges
Users which are nominated to vote on outcomes of predictions. 

## Votes
A decision on the outcome of a prediction by a single judge. 

## Market
A process for a single prediction where users can place bets on prediction outcomes which get decided by votes from judges. 

## Judge Share
A portion of the market funds that is given to the voting judges if the market resolves successfully. 

## Decision Period
The length of the decision period phase in which the judges vote on the outcome of the prediction. 

## Trading End
A point in time as of which the outcome of a prediction should be evident. 

## Judge count
The minimal amount of judges that need to accept their nomination for a market to enter the trading phase. 


# Phases

## 1. Market Creation
A new market can be created by anyone. 
Upon market creation there are a few parameters that need to be set by the creating user:

- prediction
- nominated judges
- judge count
- judge share
- trading end
- decision period

The judge count needs to be at least 1 and the trading end needs to be in the future. 


## 2. Waiting for Judges
This phase waits for enough judges to accept their nomination. If this phase lasts past the trading end the market closes because trading never started. 
The judge count determines how many judges need to accept their nomination for the trading phase to start. 
All judges that are left once the judge count was reached don't get to take part in voting. 

## 3. Trading
In this phase users can place and cancel bets. 
It lasts until the trading end is reached. 

## 4. Voting
This phase is used to let judges vote on the outcome of the prediction. 
It ends under two circumstances:

- all judges that get to vote have voted
- the decision period ran out

If the decision period ran out then all of the market funds are refunded to the users and the market closes. 
The next phase begins of all of the votes where casted by the judges. 

## 5. Payout
This phase is for caclulating the resulting payouts for users and judges if the trading and decision making period ended successfully. 
If the votes result in a tie then all of the market funds get refunded and the market closes. 
The outcome is decided by the majority of the votes. 
The Judge share gets subtracted from the market funds and given to the judges which voted on the outcome which was also voted on by the majority of the judges. 
The rest of the market funds are payed out to the users who placed bets on the outcome that got voted on by the majority of the judges. 
The individual user payout is sized protportional to the sum of their bets on the outcome decided on by the judges. 

```
MarketFunds = JudgePayouts + UserPayouts = Bets(outcome = true) + Bets(non-outcome = false)
JudgePayouts = MarketFunds * JudgeShare
Bets(true) = Sum(UserBets(true))
UserShare = UserPayouts * (UserBets(true) / Bets(true))
UserPayouts = Sum(UserShare)
JudgePayout = JudgePayouts / JudgeCount(true)
```
Example:
Given:

- judge share 10%
- judge A votes true
- judge B votes true
- judge C votes false
- user A bets 100 sats on true
- user B bets 100 sats on false
- user C bets 100 sats on true and 100 sats on false

Payouts: -> outcome=true

- judge A: 20 sats
- judge B: 20 sats
- user A: 180 sats
- user B: 0 sats
- user C: 180 sats


# Incentives
To attract users the platform needs to make sure that neutral and logical outcomes are voted for by judges that have a history of casting such votes. 
This needs to happen because users will only bet in markets where judges can be trusted to cast rational votes. 
The trust for deciding outcomes is therefore not on the side of the platform anymore but on the side of users which act as judges. 
To make sure that judges decide on rational outcomes the incentives need to be alligned in a way so that they can only make profit when rational decisions are made. 
Judges are incentivized by the judge share that gets subtracted from the market funds during payout calculations. 
To incentivize rational votes only judges which vote with the majority of the judges get a payout of the judge share. 
For judges to make profit long term they need to be able to vote in markets that have deep liquidity. Placing bets on markets makes only sense for users if they can expect the judges to vote rationally. 
Therefore judges can only make profit long term if they are trusted by users to make rational votes. For that they need to establish a trustworthy presence for users to observe. 
In summary judges need to make rational votes to establish trust with users to be able to vote in liquid markets to make profit. 


